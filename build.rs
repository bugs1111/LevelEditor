use std::{env, path::PathBuf, process::Command};

fn main() {
    println!("cargo:rerun-if-changed=assets/LevelEditor.ico");
    println!("cargo:rerun-if-changed=assets/LevelEditor.rc");
    let target = env::var("TARGET").unwrap();
    if !target.contains("windows") {
        return;
    }
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
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
