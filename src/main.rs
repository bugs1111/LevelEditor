#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app;
mod assets;
mod canvas;
mod mcp;
mod platform_files;
mod theme;
mod updater;
mod viewport;

use std::path::PathBuf;

#[cfg(target_os = "macos")]
const APP_ICON: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/macos-app-icon.png"));
#[cfg(not(target_os = "macos"))]
const APP_ICON: &[u8] = include_bytes!("../assets/LevelEditor.png");

fn main() -> eframe::Result {
    let mut args = std::env::args_os().skip(1);
    let first = args.next();
    if first.as_deref() == Some(std::ffi::OsStr::new("--mcp")) {
        let result = match (args.next(), args.next()) {
            (Some(path), None) => mcp::run(PathBuf::from(path)),
            _ => Err("Usage: level-editor --mcp <project.ldtk>".into()),
        };
        if let Err(error) = result {
            eprintln!("{error}");
            std::process::exit(1);
        }
        return Ok(());
    }
    if let Some(result) = updater::apply_update_cli() {
        if let Err(error) = result {
            eprintln!("{error}");
            std::process::exit(1);
        }
        return Ok(());
    }
    let _open_files: platform_files::OpenFilesGuard = platform_files::install();
    let path = first.map(PathBuf::from);
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            // Match the original's normal window bounds when maximization is unavailable.
            .with_inner_size([800.0, 600.0])
            .with_maximized(true)
            .with_app_id("io.ldtk.rust-port")
            .with_icon(
                eframe::icon_data::from_png_bytes(APP_ICON).expect("bundled Level Editor icon"),
            )
            .with_title("Level Editor"),
        renderer: eframe::Renderer::Wgpu,
        wgpu_options: app::native_gpu_config(),
        #[cfg(target_os = "macos")]
        event_loop_builder: Some(Box::new(platform_files::configure_quit)),
        ..Default::default()
    };
    eframe::run_native(
        "Level Editor",
        options,
        Box::new(move |cc| {
            theme::configure(&cc.egui_ctx);
            Ok(Box::new(app::EditorApp::new(cc, path)))
        }),
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn native_binary_enables_a_platform_gpu_backend() {
        let backends = eframe::wgpu::Instance::enabled_backend_features();
        #[cfg(target_os = "macos")]
        assert!(backends.contains(eframe::wgpu::Backends::METAL));
        #[cfg(target_os = "windows")]
        assert!(backends.contains(eframe::wgpu::Backends::DX12));
        #[cfg(target_os = "linux")]
        assert!(backends.contains(eframe::wgpu::Backends::VULKAN));
    }
}
