//! Real native capture and input timing harness; controls exist only in this example.
#[path = "../src/app.rs"]
mod app;
#[path = "../src/assets.rs"]
mod assets;
#[path = "../src/canvas.rs"]
mod canvas;
#[path = "../src/platform_files.rs"]
mod platform_files;
#[path = "../src/theme.rs"]
mod theme;
#[path = "../src/updater.rs"]
mod updater;
#[path = "../src/viewport.rs"]
mod viewport;
use eframe::egui;
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::Instant,
};
static CAPTURED: AtomicBool = AtomicBool::new(false);

#[derive(Default)]
struct SurfaceErrors([AtomicU64; 5]);
impl SurfaceErrors {
    fn record(&self, error: &eframe::wgpu::SurfaceError) {
        use eframe::wgpu::SurfaceError;
        let index = match error {
            SurfaceError::Timeout => 0,
            SurfaceError::Outdated => 1,
            SurfaceError::Lost => 2,
            SurfaceError::OutOfMemory => 3,
            SurfaceError::Other => 4,
        };
        self.0[index].fetch_add(1, Ordering::Relaxed);
    }
    fn snapshot(&self) -> [u64; 5] {
        std::array::from_fn(|i| self.0[i].load(Ordering::Relaxed))
    }
    fn print(&self) {
        let [timeout, outdated, lost, out_of_memory, other] = self.snapshot();
        eprintln!(
            "Surface acquire errors (all frames): {}",
            serde_json::json!({"Timeout":timeout,"Outdated":outdated,"Lost":lost,"OutOfMemory":out_of_memory,"Other":other})
        );
    }
}
fn observe_surface_errors(config: &mut eframe::egui_wgpu::WgpuConfiguration) -> Arc<SurfaceErrors> {
    let counts = Arc::new(SurfaceErrors::default());
    let observed = counts.clone();
    let original = config.on_surface_error.clone();
    config.on_surface_error = Arc::new(move |error| {
        observed.record(&error);
        original(error)
    });
    counts
}

struct Smoke {
    app: app::EditorApp,
    frames: u32,
    started: Instant,
    captured: bool,
    ready_frames: u32,
    ready_since: Option<Instant>,
    measurement_started: Option<Instant>,
    requested: bool,
    home: bool,
    shortcut: Option<egui::Key>,
    view: Option<CaptureView>,
    view_applied: bool,
    initial_inputs_done: bool,
    focus_sent: bool,
    pointer: Option<egui::Pos2>,
    stroke: Vec<egui::Pos2>,
    actions: Vec<CaptureAction>,
    action_frame: Option<u32>,
    actions_done: bool,
    action_modifiers: egui::Modifiers,
    stroke_step: Option<usize>,
    stroke_done: bool,
    input_frame: bool,
    released_this_frame: bool,
    stable_frames: u32,
    settle_seconds: f64,
    ready_update_times: Vec<f64>,
    input_update_times: Vec<f64>,
    ready_rss: Option<u64>,
    last_update_started: Option<Instant>,
    eframe_cpu_times: Vec<f64>,
    update_intervals: Vec<f64>,
    input_intervals: Vec<f64>,
    surface_errors: Arc<SurfaceErrors>,
}
fn rss_kib() -> Option<u64> {
    let output = std::process::Command::new("ps")
        .args(["-o", "rss=", "-p", &std::process::id().to_string()])
        .output()
        .ok()?;
    std::str::from_utf8(&output.stdout)
        .ok()?
        .trim()
        .parse()
        .ok()
}
fn timing(label: &str, samples: &mut [f64]) {
    samples.sort_by(f64::total_cmp);
    let n = samples.len();
    if n == 0 {
        return;
    }
    eprintln!(
        "{label}: {n} samples; median {:.3}ms; p95 {:.3}ms; max {:.3}ms",
        samples[n / 2],
        samples[(n * 95 / 100).min(n - 1)],
        samples[n - 1]
    );
}
impl Smoke {
    fn inject_raw_input(&mut self, _ctx: &egui::Context, input: &mut egui::RawInput) {
        // This deterministic input scenario simulates a foreground window.
        // Otherwise winit's unfocused input reverses the injected focus event
        // on the following frame when run as an unbundled test executable.
        input.focused = true;
        input
            .events
            .retain(|event| !matches!(event, egui::Event::WindowFocused(false)));
        discard_external_input(input);
        self.input_frame = false;
        self.released_this_frame = false;
        if !self.view_applied || !(self.app.capture_ready() || self.home) {
            return;
        }
        if !self.focus_sent {
            input.focused = true;
            input.events.push(egui::Event::WindowFocused(true));
            self.focus_sent = true;
            return;
        }
        if !self.initial_inputs_done {
            if self.app.capture_state()["clicktrap"] == true {
                return; // source onAppFocus dismisses its mask after two updates.
            }
            if let Some(pointer) = self.pointer {
                input.events.push(egui::Event::PointerMoved(pointer));
            }
            if let Some(key) = self.shortcut.take() {
                for pressed in [true, false] {
                    input.events.push(egui::Event::Key {
                        key,
                        physical_key: Some(key),
                        pressed,
                        repeat: false,
                        modifiers: egui::Modifiers::NONE,
                    });
                }
                eprintln!("Native smoke injected shortcut: {key:?}");
            }
            self.initial_inputs_done = true;
            self.ready_frames = 0;
            return;
        }
        if !self.actions.is_empty() {
            if self.action_frame.is_none() && self.input_ready() {
                eprintln!("Workflow before: {}", self.app.capture_state());
                self.action_frame = Some(0);
                self.ready_frames = 0;
            }
            if let Some(frame) = self.action_frame {
                for action in self.actions.iter().filter(|action| action.frame == frame) {
                    input.events.extend(action.events.iter().cloned());
                    self.action_modifiers = action.modifiers;
                    self.input_frame = true;
                    eprintln!("Workflow frame {frame}: {:?}", action.events);
                }
                input.modifiers = self.action_modifiers;
                self.action_frame = Some(frame + 1);
                if !self.actions_done && frame >= self.actions.last().unwrap().frame {
                    self.actions_done = true;
                    self.released_this_frame = true;
                    self.ready_frames = 0;
                }
            }
        }
        if !self.stroke.is_empty() && self.stroke_step.is_none() && self.input_ready() {
            eprintln!("Input before: {}", self.app.capture_state());
            self.stroke_step = Some(0);
            self.ready_frames = 0;
        }
        if let Some(step) = self.stroke_step
            && step <= self.stroke.len()
        {
            input.events.extend(stroke_events(&self.stroke, step));
            self.input_frame = true;
            self.stroke_step = Some(step + 1);
            if step == self.stroke.len() {
                self.stroke_done = true;
                self.released_this_frame = true;
                self.ready_frames = 0;
            }
        }
    }
    fn input_ready(&self) -> bool {
        self.ready_frames >= 60
            && self
                .ready_since
                .is_some_and(|since| since.elapsed().as_secs_f64() >= 1.)
    }
}
impl eframe::App for Smoke {
    fn raw_input_hook(&mut self, ctx: &egui::Context, input: &mut egui::RawInput) {
        self.inject_raw_input(ctx, input);
        self.app.raw_input_hook(ctx, input);
    }
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        if self.captured {
            std::process::exit(0);
        }
        if !self.view_applied && (self.app.capture_ready() || self.home) {
            if let Some(view) = self.view.take() {
                self.app
                    .set_capture_view(view.center, view.zoom, view.world);
            }
            self.view_applied = true;
            self.ready_frames = 0;
            eprintln!("Capture initialized: {}", self.app.capture_state());
        }
        let started = Instant::now();
        let interval = self
            .last_update_started
            .replace(started)
            .map(|previous| started.duration_since(previous).as_secs_f64() * 1000.);
        self.app.update(ctx, frame);
        let update_ms = started.elapsed().as_secs_f64() * 1000.;
        self.frames += 1;
        if self.input_frame {
            self.input_update_times.push(update_ms);
            if let Some(interval) = interval {
                self.input_intervals.push(interval);
            }
        }
        if self.released_this_frame {
            eprintln!("Input after: {}", self.app.capture_state());
        }
        if self.initial_inputs_done && (self.app.capture_ready() || (self.home && self.frames > 1))
        {
            self.ready_frames += 1;
            if self.ready_frames == 1 {
                self.ready_since = Some(Instant::now());
                self.measurement_started = None;
                self.ready_update_times.clear();
                self.eframe_cpu_times.clear();
                self.update_intervals.clear();
            }
            // RSS sampling at frame 20 starts a process. Its work falls outside
            // app.update, but contaminates the next wall interval and the
            // previous-frame eframe CPU observation. Start after both frames.
            // Stop before screenshot readback can affect subsequent frames.
            if self.ready_frames >= 22
                && self
                    .ready_since
                    .is_some_and(|since| since.elapsed().as_secs_f64() >= self.settle_seconds)
                && !self.input_frame
                && !self.requested
            {
                self.measurement_started.get_or_insert_with(Instant::now);
                self.ready_update_times.push(update_ms);
                if let Some(cpu) = frame.info().cpu_usage {
                    self.eframe_cpu_times.push(f64::from(cpu) * 1000.);
                }
                if let Some(interval) = interval {
                    self.update_intervals.push(interval);
                }
            }
            if self.ready_frames == 20 && self.ready_rss.is_none() {
                self.ready_rss = rss_kib();
                self.last_update_started = None;
            }
        } else {
            self.ready_frames = 0;
            self.ready_since = None;
            self.measurement_started = None;
            self.ready_update_times.clear();
            self.eframe_cpu_times.clear();
            self.update_intervals.clear();
        }
        for event in ctx.input(|input| input.events.clone()) {
            if let egui::Event::Screenshot { image, .. } = event {
                if self.app.capture_state()["clicktrap"] == true {
                    eprintln!("Native smoke FAILED: captured an unfocused clicktrap mask");
                    std::process::exit(1);
                }
                let bytes = image
                    .pixels
                    .iter()
                    .flat_map(|pixel| pixel.to_array())
                    .collect::<Vec<_>>();
                let target = std::env::var_os("LDTK_SMOKE_OUTPUT")
                    .map(PathBuf::from)
                    .unwrap_or_else(|| {
                        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                            .join("docs/status/native-smoke.png")
                    });
                match image::save_buffer(
                    &target,
                    &bytes,
                    image.width() as u32,
                    image.height() as u32,
                    image::ColorType::Rgba8,
                ) {
                    Ok(()) => {
                        self.captured = true;
                        CAPTURED.store(true, Ordering::Relaxed);
                        let size = ctx.viewport_rect().size();
                        eprintln!(
                            "Capture viewport: {}",
                            serde_json::json!({"logicalSize":[size.x,size.y],"pixelsPerPoint":ctx.pixels_per_point()})
                        );
                        timing("Ready app.update CPU", &mut self.ready_update_times);
                        timing(
                            "Ready eframe CPU (previous frame update + CPU rendering)",
                            &mut self.eframe_cpu_times,
                        );
                        timing(
                            "Ready update-start wall interval",
                            &mut self.update_intervals,
                        );
                        timing(
                            "Injected input update-start wall interval",
                            &mut self.input_intervals,
                        );
                        timing(
                            "Injected input app.update CPU",
                            &mut self.input_update_times,
                        );
                        self.surface_errors.print();
                        eprintln!(
                            "RSS KiB: ready={:?}; capture={:?}",
                            self.ready_rss,
                            rss_kib()
                        );
                        eprintln!("Capture final: {}", self.app.capture_state());
                        eprintln!(
                            "Capture cursor: {:?}",
                            ctx.output(|output| output.cursor_icon)
                        );
                        eprintln!("Screenshot saved: {}", target.display());
                    }
                    Err(error) => {
                        eprintln!("Screenshot failed: {error}");
                        std::process::exit(1);
                    }
                }
            }
        }
        if self.ready_frames >= self.stable_frames
            && self
                .measurement_started
                .is_some_and(|since| since.elapsed().as_secs_f64() >= 2.)
            && (self.stroke.is_empty() || self.stroke_done)
            && (self.actions.is_empty() || self.actions_done)
            && !self.requested
        {
            eprintln!(
                "Ready sampling window: {:.3}s after {:.3}s settling",
                self.measurement_started.unwrap().elapsed().as_secs_f64(),
                self.settle_seconds
            );
            self.requested = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
        }
        if self.started.elapsed().as_secs() > 60 && !self.captured {
            self.surface_errors.print();
            eprintln!(
                "Native smoke FAILED: no verified capture after 60 seconds ({} stable frames); state={}",
                self.ready_frames,
                self.app.capture_state()
            );
            std::process::exit(1);
        }
        if self.captured {
            eprintln!(
                "Native smoke: {} frames in {:.3}s; screenshot=true",
                self.frames,
                self.started.elapsed().as_secs_f64()
            );
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        } else {
            ctx.request_repaint();
        }
    }
}
fn shortcut(raw: &str) -> egui::Key {
    match raw {
        "Escape" => egui::Key::Escape,
        "Space" => egui::Key::Space,
        "A" => egui::Key::A,
        "B" => egui::Key::B,
        "V" => egui::Key::V,
        "S" => egui::Key::S,
        "E" => egui::Key::E,
        "P" => egui::Key::P,
        "C" => egui::Key::C,
        "L" => egui::Key::L,
        "T" => egui::Key::T,
        "U" => egui::Key::U,
        "H" => egui::Key::H,
        "Backtick" | "`" => egui::Key::Backtick,
        "F1" => egui::Key::F1,
        "F2" => egui::Key::F2,
        "F3" => egui::Key::F3,
        "F4" => egui::Key::F4,
        "F5" => egui::Key::F5,
        "F6" => egui::Key::F6,
        "F7" => egui::Key::F7,
        "F8" => egui::Key::F8,
        "F9" => egui::Key::F9,
        _ => panic!("Unsupported LDTK_SMOKE_KEY: {raw}"),
    }
}
fn main() -> eframe::Result {
    let _open_files: platform_files::OpenFilesGuard = platform_files::install();
    let path = std::env::args_os().nth(1).map(PathBuf::from);
    let home = path.is_none();
    let shortcut = std::env::var("LDTK_SMOKE_KEY")
        .ok()
        .map(|value| shortcut(&value));
    let view = std::env::var("LDTK_SMOKE_VIEW").ok().map(|value| {
        parse_view(&value).expect("LDTK_SMOKE_VIEW must be [centerX,centerY,zoom,world]")
    });
    let pointer = std::env::var("LDTK_SMOKE_POINTER").ok().map(|value| {
        let point: [f32; 2] =
            serde_json::from_str(&value).expect("LDTK_SMOKE_POINTER must be [screenX,screenY]");
        assert!(point.iter().all(|n| n.is_finite()));
        egui::Pos2::from(point)
    });
    let stroke: Vec<egui::Pos2> = std::env::var("LDTK_SMOKE_STROKE")
        .ok()
        .map(|value| {
            let points: Vec<[f32; 2]> = serde_json::from_str(&value)
                .expect("LDTK_SMOKE_STROKE must be [[screenX,screenY],...]");
            assert!(points.len() >= 2 && points.iter().flatten().all(|n| n.is_finite()));
            points.into_iter().map(egui::Pos2::from).collect()
        })
        .unwrap_or_default();
    let actions = std::env::var("LDTK_SMOKE_ACTIONS")
        .ok()
        .map(|raw| parse_actions(&raw).expect("invalid LDTK_SMOKE_ACTIONS"))
        .unwrap_or_default();
    assert!(
        actions.is_empty() || stroke.is_empty(),
        "use actions or stroke, not both"
    );
    let stable_frames = std::env::var("LDTK_SMOKE_FRAMES")
        .ok()
        .map(|value| {
            value
                .parse::<u32>()
                .expect("LDTK_SMOKE_FRAMES must be an integer")
                .max(60)
        })
        .unwrap_or(60);
    let settle_seconds = std::env::var("LDTK_SMOKE_SETTLE_SECONDS")
        .ok()
        .map(|value| {
            value
                .parse::<f64>()
                .expect("LDTK_SMOKE_SETTLE_SECONDS must be a number")
        })
        .unwrap_or(1.);
    assert!(settle_seconds.is_finite() && settle_seconds >= 1.);
    let mut wgpu_options = app::native_gpu_config();
    let surface_errors = observe_surface_errors(&mut wgpu_options);
    let final_errors = surface_errors.clone();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1440., 900.]),
        renderer: eframe::Renderer::Wgpu,
        wgpu_options,
        #[cfg(target_os = "macos")]
        event_loop_builder: Some(Box::new(platform_files::configure_quit)),
        ..Default::default()
    };
    let result = eframe::run_native(
        "LevelEditor runtime verification",
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
                ready_frames: 0,
                ready_since: None,
                measurement_started: None,
                requested: false,
                home,
                shortcut,
                view,
                view_applied: false,
                initial_inputs_done: false,
                focus_sent: false,
                pointer,
                stroke,
                actions,
                action_frame: None,
                actions_done: false,
                action_modifiers: egui::Modifiers::NONE,
                stroke_step: None,
                stroke_done: false,
                input_frame: false,
                released_this_frame: false,
                stable_frames,
                settle_seconds,
                ready_update_times: Vec::new(),
                input_update_times: Vec::new(),
                ready_rss: None,
                last_update_started: None,
                eframe_cpu_times: Vec::new(),
                update_intervals: Vec::new(),
                input_intervals: Vec::new(),
                surface_errors,
            }))
        }),
    );
    if !CAPTURED.load(Ordering::Relaxed) {
        final_errors.print();
        if result.is_ok() {
            std::process::exit(1);
        }
    }
    result
}
#[derive(Debug, PartialEq)]
struct CaptureView {
    center: [f32; 2],
    zoom: f32,
    world: bool,
}
fn parse_view(raw: &str) -> Result<CaptureView, String> {
    let value: serde_json::Value = serde_json::from_str(raw).map_err(|error| error.to_string())?;
    let values = value
        .as_array()
        .filter(|a| a.len() == 4)
        .ok_or("expected four values")?;
    let number = |index: usize| {
        values[index]
            .as_f64()
            .map(|value| value as f32)
            .filter(|value| value.is_finite())
            .ok_or_else(|| "expected finite coordinates/zoom".to_owned())
    };
    let view = CaptureView {
        center: [number(0)?, number(1)?],
        zoom: number(2)?,
        world: values[3].as_bool().ok_or("world must be boolean")?,
    };
    if view.zoom <= 0. {
        return Err("zoom must be positive".into());
    }
    Ok(view)
}
fn stroke_events(points: &[egui::Pos2], step: usize) -> Vec<egui::Event> {
    if points.is_empty() {
        return Vec::new();
    }
    let button = |pos, pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    };
    match step {
        0 => vec![
            egui::Event::PointerMoved(points[0]),
            button(points[0], true),
        ],
        index if index < points.len() => vec![egui::Event::PointerMoved(points[index])],
        index if index == points.len() => vec![button(points[index - 1], false)],
        _ => Vec::new(),
    }
}

fn discard_external_input(input: &mut egui::RawInput) {
    // Preserve capture delivery and explicit focus notifications. All editor
    // interactions in this isolated example come from the scripted scenario.
    input.events.retain(|event| {
        matches!(
            event,
            egui::Event::Screenshot { .. } | egui::Event::WindowFocused(true)
        )
    });
    input.modifiers = egui::Modifiers::NONE;
}
fn parse_actions(raw: &str) -> Result<Vec<CaptureAction>, String> {
    let values: Vec<serde_json::Value> = serde_json::from_str(raw).map_err(|e| e.to_string())?;
    let mut actions: Vec<CaptureAction> = Vec::new();
    for value in values {
        let frame = value["frame"]
            .as_u64()
            .and_then(|n| u32::try_from(n).ok())
            .ok_or("frame must be u32")?;
        if actions.last().is_some_and(|a| a.frame > frame) {
            return Err("frames must be ordered".into());
        }
        let modifiers = egui::Modifiers {
            shift: value["shift"] == true,
            alt: value["alt"] == true,
            ctrl: value["ctrl"] == true,
            command: value["command"] == true,
            mac_cmd: value["command"] == true,
        };
        let mut events = Vec::new();
        let pointer = if let Some(point) = value.get("pointer") {
            let p: [f32; 2] = serde_json::from_value(point.clone()).map_err(|e| e.to_string())?;
            if !p.iter().all(|n| n.is_finite()) {
                return Err("pointer must be finite".into());
            }
            let p = egui::Pos2::from(p);
            events.push(egui::Event::PointerMoved(p));
            Some(p)
        } else {
            None
        };
        if let Some(button) = value["button"].as_str() {
            events.push(egui::Event::PointerButton {
                pos: pointer.ok_or("button requires pointer")?,
                button: match button {
                    "primary" => egui::PointerButton::Primary,
                    "secondary" => egui::PointerButton::Secondary,
                    _ => return Err("unknown button".into()),
                },
                pressed: value["pressed"]
                    .as_bool()
                    .ok_or("button requires pressed")?,
                modifiers,
            });
        }
        if let Some(key) = value["key"].as_str() {
            let key = shortcut(key);
            events.push(egui::Event::Key {
                key,
                physical_key: Some(key),
                pressed: value["pressed"].as_bool().ok_or("key requires pressed")?,
                repeat: false,
                modifiers,
            });
        }
        actions.push(CaptureAction {
            frame,
            events,
            modifiers,
        });
    }
    Ok(actions)
}
struct CaptureAction {
    frame: u32,
    events: Vec<egui::Event>,
    modifiers: egui::Modifiers,
}
#[cfg(test)]
mod capture_tests {
    use super::*;
    #[test]
    fn surface_error_observation_counts_each_kind_without_changing_handler_actions() {
        use eframe::{egui_wgpu::SurfaceErrorAction, wgpu::SurfaceError};
        let delegated = Arc::new(AtomicU64::new(0));
        let calls = delegated.clone();
        let mut config = eframe::egui_wgpu::WgpuConfiguration {
            on_surface_error: Arc::new(move |error| {
                calls.fetch_add(1, Ordering::Relaxed);
                if error == SurfaceError::Outdated {
                    SurfaceErrorAction::RecreateSurface
                } else {
                    SurfaceErrorAction::SkipFrame
                }
            }),
            ..Default::default()
        };
        let counts = observe_surface_errors(&mut config);
        for error in [
            SurfaceError::Timeout,
            SurfaceError::Outdated,
            SurfaceError::Lost,
            SurfaceError::OutOfMemory,
            SurfaceError::Other,
        ] {
            let recreate = error == SurfaceError::Outdated;
            let action = (config.on_surface_error)(error);
            assert_eq!(
                matches!(action, SurfaceErrorAction::RecreateSurface),
                recreate
            );
        }
        assert_eq!(counts.snapshot(), [1, 1, 1, 1, 1]);
        assert_eq!(delegated.load(Ordering::Relaxed), 5);
    }
    #[test]
    fn view_parameters_require_finite_coordinates_positive_zoom_and_explicit_mode() {
        assert_eq!(
            parse_view("[408,232,1,false]").unwrap(),
            CaptureView {
                center: [408., 232.],
                zoom: 1.,
                world: false
            }
        );
        for invalid in [
            "[]",
            "[0,0,0,false]",
            "[0,0,-1,true]",
            "[0,0,1,1]",
            "[1e100,0,1,false]",
        ] {
            assert!(parse_view(invalid).is_err(), "{invalid}");
        }
    }
    #[test]
    fn deterministic_pointer_rejects_os_move_and_release() {
        let mut input = egui::RawInput {
            events: stroke_events(&[egui::pos2(900., 20.)], 1),
            ..Default::default()
        };
        input
            .events
            .push(egui::Event::PointerMoved(egui::pos2(900., 20.)));
        input.events.push(egui::Event::PointerGone);
        input.events.push(egui::Event::Text("external".into()));
        input.events.push(egui::Event::WindowFocused(true));
        discard_external_input(&mut input);
        assert!(matches!(
            &input.events[..],
            [egui::Event::WindowFocused(true)]
        ));
    }
    #[test]
    fn workflow_actions_preserve_held_drag_and_modifiers() {
        let actions = parse_actions(r#"[{"frame":0,"pointer":[500,300],"button":"primary","pressed":true,"shift":true},{"frame":5,"pointer":[540,320],"shift":true}]"#).unwrap();
        assert_eq!(actions.len(), 2);
        assert_eq!(actions[1].frame, 5);
        assert!(actions[0].modifiers.shift);
        assert!(matches!(
            &actions[0].events[..],
            [
                egui::Event::PointerMoved(_),
                egui::Event::PointerButton { pressed: true, .. }
            ]
        ));
        assert!(matches!(
            &actions[1].events[..],
            [egui::Event::PointerMoved(_)]
        ));
        assert!(parse_actions(r#"[{"frame":4},{"frame":1}]"#).is_err());
        assert!(parse_actions(r#"[{"frame":0,"pointer":[1e100,0]}]"#).is_err());
    }
    #[test]
    fn scripted_stroke_presses_moves_and_releases_on_separate_frames() {
        let points = [egui::pos2(400., 300.), egui::pos2(416., 300.)];
        assert!(matches!(
            &stroke_events(&points, 0)[..],
            [
                egui::Event::PointerMoved(_),
                egui::Event::PointerButton { pressed: true, .. }
            ]
        ));
        assert!(matches!(
            &stroke_events(&points, 1)[..],
            [egui::Event::PointerMoved(_)]
        ));
        assert!(matches!(
            &stroke_events(&points, 2)[..],
            [egui::Event::PointerButton { pressed: false, .. }]
        ));
        assert!(stroke_events(&points, 3).is_empty());
    }
}
