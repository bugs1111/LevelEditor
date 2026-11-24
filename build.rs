use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

fn main() {
    println!("cargo:rerun-if-changed=assets/LevelEditor.ico");
    println!("cargo:rerun-if-changed=assets/LevelEditor.rc");
    let target = env::var("TARGET").unwrap();
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    if target.ends_with("-apple-darwin") {
        println!("cargo:rerun-if-changed=assets/LevelEditor.icns");
        write_macos_app_icon(&out);
        return;
    }
    if !target.contains("windows") {
        return;
    }
    let (mut command, resource) = if target.ends_with("msvc") {
        let resource = out.join("LevelEditor.res");
        let mut command = cc::windows_registry::find_tool(&target, "rc.exe")
            .expect("Windows resource compiler not found")
            .to_command();
        command.arg(format!("/fo{}", resource.display()));
        (command, resource)
    } else {
        let resource = out.join("LevelEditor.o");
        let mut command = Command::new(format!(
            "{}-w64-mingw32-windres",
            target.split('-').next().unwrap()
        ));
        command.args(["-O", "coff", "-o"]).arg(&resource);
        (command, resource)
    };
    let status = command
        .arg("assets/LevelEditor.rc")
        .status()
        .expect("run resource compiler");
    assert!(status.success(), "resource compiler failed: {status}");
    println!("cargo:rustc-link-arg-bin=ldtk-rs={}", resource.display());
}

fn write_macos_app_icon(out: &Path) {
    let icon = fs::read("assets/LevelEditor.icns").expect("read macOS app icon");
    assert!(
        icon.starts_with(b"icns") && icon.len() >= 8,
        "invalid ICNS header"
    );
    let mut entries = &icon[8..];
    while !entries.is_empty() {
        let header = entries.get(..8).expect("truncated ICNS entry header");
        let size = u32::from_be_bytes(header[4..].try_into().unwrap()) as usize;
        assert!(size >= 8, "invalid ICNS entry size");
        let (entry, rest) = entries
            .split_at_checked(size)
            .expect("truncated ICNS entry");
        if &header[..4] == b"ic10" {
            // Use the bundle's 1024px representation, including its Dock padding.
            fs::write(out.join("macos-app-icon.png"), &entry[8..])
                .expect("write macOS runtime icon");
            return;
        }
        entries = rest;
    }
    panic!("ICNS is missing its 1024px icon");
}
