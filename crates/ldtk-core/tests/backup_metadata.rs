use ldtk_core::Project;
use std::{
    alloc::{GlobalAlloc, Layout, System},
    fs::{self, File},
    io::Write,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
};

struct AllocationCheck;
static TRACK: AtomicBool = AtomicBool::new(false);
static LARGEST: AtomicUsize = AtomicUsize::new(0);
unsafe impl GlobalAlloc for AllocationCheck {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if TRACK.load(Ordering::Relaxed) {
            LARGEST.fetch_max(layout.size(), Ordering::Relaxed);
        }
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        unsafe { System.dealloc(pointer, layout) }
    }
    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        if TRACK.load(Ordering::Relaxed) {
            LARGEST.fetch_max(size, Ordering::Relaxed);
        }
        unsafe { System.realloc(pointer, layout, size) }
    }
}
#[global_allocator]
static ALLOCATOR: AllocationCheck = AllocationCheck;

#[test]
fn metadata_discovery_streams_large_unused_strings_in_the_project_and_each_backup() {
    fn write_large(path: &std::path::Path, tail: &[u8]) {
        let mut file = File::create(path).unwrap();
        file.write_all(b"{\"levels\":\"").unwrap();
        let chunk = [b'x'; 8192];
        for _ in 0..2048 {
            file.write_all(&chunk).unwrap();
        }
        file.write_all(tail).unwrap();
    }
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("World.ldtk");
    let backup = dir.path().join("backups/project-id_2026-11-17_01-02-03");
    fs::create_dir_all(&backup).unwrap();
    write_large(
        &path,
        b"\",\"iid\":\"project-id\",\"backupRelPath\":\"backups\"}",
    );
    write_large(&backup.join("World.ldtk"), b"\",\"iid\":\"project-id\"}");
    LARGEST.store(0, Ordering::Relaxed);
    TRACK.store(true, Ordering::SeqCst);
    let result = Project::list_backups_at(&path);
    TRACK.store(false, Ordering::SeqCst);
    let largest = LARGEST.load(Ordering::Relaxed);
    let entries = result.unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].path, backup.join("World.ldtk"));
    assert!(
        largest < 256 * 1024,
        "metadata discovery allocated {largest} bytes for an unused 16 MiB string"
    );
}

#[test]
fn metadata_discovery_preserves_legacy_names_invalid_iid_fallback_sorting_and_type_rules() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("World.ldtk");
    fs::write(
        &path,
        r#"{"iid":"project-id","backupRelPath":17,"levels":[{"externalRelPath":"missing.ldtkl"}]}"#,
    )
    .unwrap();
    let root = dir.path().join("World/backups");
    let fixtures = [
        (
            "backup_2026-11-17_01-00-00",
            "legacy.JSON",
            r#"{"iid":"project-id"}"#,
            true,
        ),
        (
            "project-id_2026-11-17_02-00-00_crash_0001",
            "corrupt.ldtk",
            "[unclosed",
            true,
        ),
        (
            "project-id_2026-11-17_03-00-00",
            "wrong.ldtk",
            r#"{"iid":17}"#,
            false,
        ),
        (
            "project-id_2026-11-17_04-00-00",
            "array.ldtk",
            r#"["project-id"]"#,
            false,
        ),
        (
            "backup_2026-11-17_05-00-00",
            "corrupt.ldtk",
            "{corrupt",
            false,
        ),
        (
            "other-id_2026-11-17_06-00-00",
            "other.ldtk",
            r#"{"iid":"project-id"}"#,
            false,
        ),
        (
            "project-id_2026-11-17_07-00-00",
            "last.ldtk",
            r#"{"iid":"other","iid":"project-id","backupRelPath":{}}"#,
            true,
        ),
    ];
    for (folder, name, json, _) in fixtures {
        let folder = root.join(folder);
        fs::create_dir_all(&folder).unwrap();
        fs::write(folder.join(name), json).unwrap();
    }
    let entries = Project::list_backups_at(&path).unwrap();
    assert_eq!(entries.len(), 3);
    assert_eq!(entries[0].timestamp, "2026-11-17_07-00-00");
    assert!(entries[1].crash);
    assert!(!entries[2].crash);
    for (folder, name, _, expected) in fixtures {
        assert_eq!(
            entries
                .iter()
                .any(|entry| entry.path == root.join(folder).join(name)),
            expected,
            "{folder}/{name}"
        );
    }
}
