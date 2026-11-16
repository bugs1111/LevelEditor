mod app;
mod assets;
mod canvas;
mod fields;
mod theme;
mod viewport;

use std::path::PathBuf;

fn main() -> eframe::Result {
    let path = std::env::args_os().nth(1).map(PathBuf::from);
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1440.0, 900.0])
            .with_min_inner_size([900.0, 600.0])
            .with_title("LDtk • Rust"),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    eframe::run_native(
        "LDtk • Rust",
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
