//! Runtime verification harness. It exercises the real app and renderer, then exits.
#[path = "../src/app.rs"]
mod app;
#[path = "../src/assets.rs"]
mod assets;
#[path = "../src/canvas.rs"]
mod canvas;
#[path = "../src/fields.rs"]
mod fields;
#[path = "../src/theme.rs"]
mod theme;
#[path = "../src/viewport.rs"]
mod viewport;

use eframe::egui;
use std::{path::PathBuf, time::Instant};

struct Smoke {
    app: app::EditorApp,
    frames: u32,
    started: Instant,
    captured: bool,
}

impl eframe::App for Smoke {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.app.update(ctx, frame);
        self.frames += 1;
        for event in ctx.input(|i| i.events.clone()) {
            if let egui::Event::Screenshot { image, .. } = event {
                let bytes = image
                    .pixels
                    .iter()
                    .flat_map(|p| p.to_array())
                    .collect::<Vec<_>>();
                let target =
                    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("docs/status/native-smoke.png");
                match image::save_buffer(
                    &target,
                    &bytes,
                    image.width() as u32,
                    image.height() as u32,
                    image::ColorType::Rgba8,
                ) {
                    Ok(()) => {
                        self.captured = true;
                        eprintln!("Screenshot saved: {}", target.display());
                    }
                    Err(e) => eprintln!("Screenshot failed: {e}"),
                }
            }
        }
        if self.frames == 60 {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
        }
        if self.captured || self.frames >= 180 || self.started.elapsed().as_secs() > 15 {
            eprintln!(
                "Native smoke: {} frames in {:.3}s; screenshot={}",
                self.frames,
                self.started.elapsed().as_secs_f64(),
                self.captured
            );
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        } else {
            ctx.request_repaint();
        }
    }
}

fn main() -> eframe::Result {
    let path = std::env::args_os().nth(1).map(PathBuf::from);
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1440.0, 900.0]),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };
    eframe::run_native(
        "LDtk Rust runtime verification",
        options,
        Box::new(move |cc| {
            theme::configure(&cc.egui_ctx);
            if let Some(state) = &cc.wgpu_render_state {
                eprintln!("GPU adapter: {:?}", state.adapter.get_info());
            }
            Ok(Box::new(Smoke {
                app: app::EditorApp::new(cc, path),
                frames: 0,
                started: Instant::now(),
                captured: false,
            }))
        }),
    )
}
