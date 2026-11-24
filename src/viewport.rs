use eframe::egui::Rect;

fn backing_pixel(page: f64, origin: f64, density: f64) -> f64 {
    ((page.trunc() - origin) * density).round()
}

#[derive(Clone, Debug)]
pub struct Viewport {
    pub center: [f32; 2],
    pub zoom: f32,
}

impl Default for Viewport {
    fn default() -> Self {
        Self {
            center: [0.0; 2],
            zoom: 1.0,
        }
    }
}

impl Viewport {
    pub fn to_screen(&self, world: [f32; 2], origin: [f32; 2], size: [f32; 2]) -> [f32; 2] {
        [
            origin[0] + size[0] * 0.5 + (world[0] - self.center[0]) * self.zoom,
            origin[1] + size[1] * 0.5 + (world[1] - self.center[1]) * self.zoom,
        ]
    }
    pub fn to_world(&self, screen: [f32; 2], origin: [f32; 2], size: [f32; 2]) -> [f32; 2] {
        [
            (screen[0] - origin[0] - size[0] * 0.5) / self.zoom + self.center[0],
            (screen[1] - origin[1] - size[1] * 0.5) / self.zoom + self.center[1],
        ]
    }
    pub fn level_render_canvas_origin(&self, canvas: Rect, pixels_per_point: f32) -> [f32; 2] {
        // LevelRender.hx snaps the local root before drawing the level and its filters.
        let root: eframe::egui::Vec2 = self
            .to_screen([0.; 2], [0.; 2], canvas.size().into())
            .into();
        let snapped = (root * pixels_per_point).round() / pixels_per_point;
        (canvas.min + (snapped - root)).into()
    }
    pub fn pointer_pixel(
        &self,
        page: [f32; 2],
        canvas_origin: [f32; 2],
        canvas_size: [f32; 2],
        world_offset: [f32; 2],
        pixels_per_point: f32,
    ) -> [f32; 2] {
        // Coords.hx uses the snapped physical-pixel origin of WorldRender.
        let density = f64::from(pixels_per_point);
        let zoom = f64::from(self.zoom) * density;
        std::array::from_fn(|axis| {
            let center = f64::from(self.center[axis]) + f64::from(world_offset[axis]);
            let root = (f64::from(canvas_size[axis]) * density * 0.5 - center * zoom).round();
            let pixel = backing_pixel(
                f64::from(page[axis]),
                f64::from(canvas_origin[axis]),
                density,
            );
            ((pixel - root) / zoom).round() as f32
        })
    }
    fn project_level_point(
        &self,
        zoom: f32,
        canvas: Rect,
        level_origin: [f32; 2],
        point: [f32; 2],
        pixels_per_point: f32,
    ) -> [f64; 2] {
        let density = f64::from(pixels_per_point);
        let zoom = f64::from(zoom) * density;
        let origin: [f32; 2] = canvas.min.into();
        let size: [f32; 2] = canvas.size().into();
        std::array::from_fn(|axis| {
            let center = f64::from(self.center[axis]) - f64::from(level_origin[axis]);
            let point = f64::from(point[axis]) - f64::from(level_origin[axis]);
            let middle = f64::from(size[axis]) * density * 0.5;
            let level_root = (middle - center * zoom).round();
            (level_root + point * zoom) / density + f64::from(origin[axis])
        })
    }

    pub fn level_point_page(
        &self,
        canvas: Rect,
        level_origin: [f32; 2],
        point: [f32; 2],
        pixels_per_point: f32,
    ) -> eframe::egui::Pos2 {
        self.project_level_point(self.zoom, canvas, level_origin, point, pixels_per_point)
            .map(|page| page.trunc() as f32)
            .into()
    }

    pub fn zoom_at_level_point(
        &mut self,
        zoom: f32,
        canvas: Rect,
        level_origin: [f32; 2],
        point: [f32; 2],
        pixels_per_point: f32,
    ) {
        let before =
            self.project_level_point(self.zoom, canvas, level_origin, point, pixels_per_point);
        let after = self.project_level_point(zoom, canvas, level_origin, point, pixels_per_point);
        let density = f64::from(pixels_per_point);
        let origin: [f32; 2] = canvas.min.into();
        for axis in 0..2 {
            let origin = f64::from(origin[axis]);
            // Camera.deltaZoomTo compares both page coordinates at the new
            // backing zoom. The world render origin cancels in the difference.
            let pixels = backing_pixel(after[axis], origin, density)
                - backing_pixel(before[axis], origin, density);
            let delta = pixels / (f64::from(zoom) * density);
            self.center[axis] += delta as f32;
        }
        self.zoom = zoom;
    }
    pub fn fit(&mut self, bounds: [f32; 4], size: [f32; 2], pixels_per_point: f32) {
        self.center = fit_center(bounds);
        self.zoom = fit_zoom(bounds, size, pixels_per_point, false)
            .clamp(0.03 / pixels_per_point, 32.0 / pixels_per_point);
    }

    pub fn fit_world(&mut self, bounds: [f32; 4], size: [f32; 2], pixels_per_point: f32) {
        self.center = fit_center(bounds);
        self.zoom = fit_zoom(bounds, size, pixels_per_point, true)
            .clamp(0.03 / pixels_per_point, 32.0 / pixels_per_point);
    }
}

fn fit_center(bounds: [f32; 4]) -> [f32; 2] {
    [(bounds[0] + bounds[2]) * 0.5, (bounds[1] + bounds[3]) * 0.5]
}

fn fit_zoom(bounds: [f32; 4], size: [f32; 2], pixels_per_point: f32, world: bool) -> f32 {
    // LDtk scales its margins with device density. The native viewport stores
    // zoom in egui points per level pixel.
    let width = (bounds[2] - bounds[0]).max(1.0);
    let height = (bounds[3] - bounds[1]).max(1.0);
    if world {
        (size[0] / (width * 1.1 + 300.0 * pixels_per_point)).min(size[1] / (height * 1.1))
    } else {
        let pad = 80.0 * pixels_per_point;
        (size[0] / (width + pad)).min(size[1] / (height + pad))
    }
}

const CAMERA_FPS: f32 = 60.0;
const CAMERA_MOTION_FRAMES: f32 = 1.8 * CAMERA_FPS;
const MAX_CAMERA_FRAME_GAP: f32 = 0.5;
const FIT_SETTLE_PIXELS: f32 = 0.25;

enum ZoomRequest {
    Fit {
        bounds: [f32; 4],
        world: bool,
    },
    WorldEntry {
        bounds: [f32; 4],
        one_level: bool,
    },
    LevelEntry {
        level_bounds: [f32; 4],
        world_bounds: [f32; 4],
        level_origin: [f32; 2],
    },
}

struct CameraTween<T> {
    target: T,
    remaining: f32,
    exact: bool,
}

impl<T> CameraTween<T> {
    fn new(target: T, exact: bool) -> Self {
        Self {
            target,
            remaining: CAMERA_MOTION_FRAMES,
            exact,
        }
    }

    fn step(&self, fraction: f32) -> f32 {
        fraction.min(if self.exact { 0.5 } else { 1.0 })
    }
}

#[derive(Default)]
pub struct CameraMotion {
    zoom_request: Option<ZoomRequest>,
    zoom: Option<CameraTween<f32>>,
    center: Option<CameraTween<[f32; 2]>>,
    last_frame: Option<u64>,
    last_tmod: Option<f32>,
    last_window_frame: Option<f64>,
    last_window_activity: Option<f64>,
    world_entry_deadline: Option<f64>,
}

impl CameraMotion {
    pub fn fit(&mut self, bounds: [f32; 4], world: bool) {
        self.center = Some(CameraTween::new(fit_center(bounds), true));
        self.zoom_request = Some(ZoomRequest::Fit { bounds, world });
    }

    pub fn scroll_to(&mut self, center: [f32; 2]) {
        self.center = Some(CameraTween::new(center, false));
    }

    pub fn enter_world(&mut self, bounds: [f32; 4], one_level: bool) {
        self.zoom_request = Some(ZoomRequest::WorldEntry { bounds, one_level });
    }

    pub fn fit_level_from_world(
        &mut self,
        level_bounds: [f32; 4],
        world_bounds: [f32; 4],
        level_origin: [f32; 2],
    ) {
        self.scroll_to(fit_center(level_bounds));
        self.zoom_request = Some(ZoomRequest::LevelEntry {
            level_bounds,
            world_bounds,
            level_origin,
        });
    }

    pub fn translate_center(&mut self, offset: [f32; 2]) {
        if let Some(center) = &mut self.center {
            center.target[0] += offset[0];
            center.target[1] += offset[1];
        }
    }

    pub fn cancel_pan(&mut self) {
        self.center = None;
    }

    pub fn cancel(&mut self) {
        self.cancel_pan();
        self.zoom_request = None;
        self.zoom = None;
    }

    pub fn is_active(&self) -> bool {
        self.zoom_request.is_some() || self.zoom.is_some() || self.center.is_some()
    }

    pub fn frame_tmod(&self, frame: u64, dt: f32) -> Option<f32> {
        if self.last_frame == Some(frame) {
            return None;
        }
        // The source Heaps timer retains the prior step after a long pause.
        Some(if dt < MAX_CAMERA_FRAME_GAP {
            dt * CAMERA_FPS
        } else {
            self.last_tmod.unwrap_or(1.0)
        })
    }

    pub fn window_dt(
        &mut self,
        ctx: &eframe::egui::Context,
        pointer_activity: bool,
    ) -> Option<f32> {
        let (now, dt, input) = ctx.input(|i| {
            let input = pointer_activity
                || i.events.iter().any(|event| {
                    !matches!(
                        event,
                        eframe::egui::Event::PointerMoved(_) | eframe::egui::Event::MouseMoved(_)
                    )
                });
            (i.time, i.unstable_dt, input)
        });
        let last_frame = self.last_window_frame.unwrap_or(now - f64::from(dt));
        let idle_for = now - *self.last_window_activity.get_or_insert(now);
        let entering_world = matches!(self.zoom_request, Some(ZoomRequest::WorldEntry { .. }))
            && self.zoom.is_none()
            && self.center.is_none();
        if entering_world && idle_for > 2. {
            // The original's pending browser timer survives a key press.
            // Canvas input can wake egui early, but the first zoom uses the
            // elapsed time of the previously scheduled canvas frame.
            let interval = if idle_for > 6. { 1. / 12. } else { 1. / 30. };
            let deadline = *self
                .world_entry_deadline
                .get_or_insert(last_frame + interval);
            if now < deadline {
                ctx.request_repaint_after(std::time::Duration::from_secs_f64(deadline - now));
                return None;
            }
        }
        let elapsed = if let Some(deadline) = self.world_entry_deadline.take()
            && entering_world
        {
            let elapsed = (now - last_frame) as f32;
            if elapsed < MAX_CAMERA_FRAME_GAP {
                elapsed
            } else {
                // A stalled native repaint can miss the idle canvas frame.
                // Preserve its interval when the measured gap would reuse
                // the previous, unrelated camera step.
                (deadline - last_frame) as f32
            }
        } else {
            dt
        };
        self.last_window_frame = Some(now);
        if input || self.is_active() {
            self.last_window_activity = Some(now);
        }
        Some(elapsed)
    }

    pub fn tick(
        &mut self,
        view: &mut Viewport,
        canvas: Rect,
        level_origin: [f32; 2],
        pixels_per_point: f32,
        frame: u64,
        dt: f32,
    ) -> bool {
        let Some(tmod) = self.frame_tmod(frame, dt) else {
            return self.is_active();
        };
        self.last_frame = Some(frame);
        self.last_tmod = Some(tmod);
        let size = canvas.size().into();
        let min_zoom = 0.03 / pixels_per_point;
        let max_zoom = 32.0 / pixels_per_point;
        if let Some(request) = self.zoom_request.take() {
            let exact = matches!(request, ZoomRequest::Fit { .. });
            let target = match request {
                ZoomRequest::Fit { bounds, world } => {
                    fit_zoom(bounds, size, pixels_per_point, world)
                }
                ZoomRequest::WorldEntry { bounds, one_level } => {
                    let fit = fit_zoom(bounds, size, pixels_per_point, true);
                    (0.8 * fit).max(0.3 / pixels_per_point) * if one_level { 0.5 } else { 1.0 }
                }
                ZoomRequest::LevelEntry {
                    level_bounds,
                    world_bounds,
                    level_origin,
                } => {
                    view.fit_world(world_bounds, size, pixels_per_point);
                    view.center[0] -= level_origin[0];
                    view.center[1] -= level_origin[1];
                    fit_zoom(level_bounds, size, pixels_per_point, false)
                }
            };
            self.zoom = Some(CameraTween::new(target, exact));
        }
        if let Some(motion) = &mut self.zoom {
            let target = if motion.exact {
                motion.target.clamp(min_zoom, max_zoom)
            } else {
                motion.target
            };
            let step = motion.step((0.22 * view.zoom * pixels_per_point).max(0.1) * tmod);
            let zoom = (view.zoom + (target - view.zoom) * step).clamp(min_zoom, max_zoom);
            let error = (target - zoom).abs();
            motion.remaining -= tmod;
            let settled = if motion.exact {
                // Measure the remaining zoom at the edge of the canvas.
                let half_extent = canvas.width().max(canvas.height()) * 0.5 * pixels_per_point;
                error * half_extent <= zoom * FIT_SETTLE_PIXELS || (step > 0.0 && zoom == view.zoom)
            } else {
                error <= 0.04 * zoom || motion.remaining <= 0.0
            };
            if motion.exact {
                // Fitting has its own center target. Keep zoom from shifting it
                // through pixel anchoring, even once the pan has settled.
                view.zoom = if settled { target } else { zoom };
            } else {
                view.zoom_at_level_point(zoom, canvas, level_origin, view.center, pixels_per_point);
            }
            if settled {
                self.zoom = None;
            }
        }
        if let Some(motion) = &mut self.center {
            let target = motion.target;
            let previous = view.center;
            let step = motion.step(0.15 * tmod);
            view.center[0] += (target[0] - view.center[0]) * step;
            view.center[1] += (target[1] - view.center[1]) * step;
            motion.remaining -= tmod;
            let error = (target[0] - view.center[0]).hypot(target[1] - view.center[1]);
            let settled = if motion.exact {
                error * view.zoom * pixels_per_point <= FIT_SETTLE_PIXELS
                    || (step > 0.0 && view.center == previous)
            } else {
                error <= 6.0 || motion.remaining <= 0.0
            };
            if settled {
                if motion.exact {
                    view.center = target;
                }
                self.center = None;
            }
        }
        self.is_active()
    }
}

pub fn stroke_cells(from: [i32; 2], to: [i32; 2]) -> Vec<[i32; 2]> {
    let (mut x, mut y) = (from[0], from[1]);
    let dx = (to[0] as i64 - x as i64).abs();
    let dy = -(to[1] as i64 - y as i64).abs();
    let sx = if x < to[0] { 1 } else { -1 };
    let sy = if y < to[1] { 1 } else { -1 };
    let mut error = dx + dy;
    let mut cells = Vec::new();
    loop {
        cells.push([x, y]);
        if [x, y] == to {
            break;
        }
        let doubled = error * 2;
        if doubled >= dy {
            error += dy;
            x += sx;
        }
        if doubled <= dx {
            error += dx;
            y += sy;
        }
    }
    cells
}

pub fn tile_uv(
    src: [f32; 2],
    tile_size: [f32; 2],
    atlas_size: [f32; 2],
    flips: i64,
) -> [[f32; 2]; 2] {
    let mut min = [src[0] / atlas_size[0], src[1] / atlas_size[1]];
    let mut max = [
        (src[0] + tile_size[0]) / atlas_size[0],
        (src[1] + tile_size[1]) / atlas_size[1],
    ];
    for axis in 0..2 {
        if flips & (1 << axis) != 0 {
            std::mem::swap(&mut min[axis], &mut max[axis]);
        }
    }
    [min, max]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near(actual: [f32; 2], expected: [f32; 2]) {
        assert!(
            (actual[0] - expected[0]).abs() < 0.001,
            "{actual:?} != {expected:?}"
        );
        assert!(
            (actual[1] - expected[1]).abs() < 0.001,
            "{actual:?} != {expected:?}"
        );
    }

    #[test]
    fn centered_world_maps_to_viewport_center() {
        let v = Viewport {
            center: [32.0, 48.0],
            zoom: 2.0,
        };
        near(
            v.to_screen([48.0, 32.0], [300.0, 40.0], [800.0, 600.0]),
            [732.0, 308.0],
        );
        near(
            v.to_world([732.0, 308.0], [300.0, 40.0], [800.0, 600.0]),
            [48.0, 32.0],
        );
    }

    #[test]
    fn source_pointer_rounds_negative_half_pixels_away_from_zero() {
        let view = Viewport {
            center: [25.25, 25.],
            zoom: 2.,
        };
        let canvas = [100.; 2];
        assert_eq!(
            view.pointer_pixel([0., -1.], [0.; 2], canvas, [0.; 2], 1.),
            [1., -1.]
        );
        assert_eq!(
            view.pointer_pixel([0., -1.], [0.; 2], canvas, [0.; 2], 2.),
            [0., -1.]
        );
    }

    #[test]
    fn zoom_keeps_cursor_world_point_stationary() {
        let mut v = Viewport {
            center: [100.0, 200.0],
            zoom: 2.0,
        };
        let canvas =
            Rect::from_min_size(eframe::egui::pos2(300., 0.), eframe::egui::vec2(800., 600.));
        v.zoom_at_level_point(4., canvas, [0.; 2], [150., 250.], 1.);
        near(
            v.to_world([800.0, 400.0], [300.0, 0.0], [800.0, 600.0]),
            [150.0, 250.0],
        );
        assert_eq!(v.zoom, 4.0);
    }

    #[test]
    fn fit_handles_worlds_with_negative_coordinates() {
        let mut v = Viewport::default();
        v.fit([-256.0, -128.0, 256.0, 128.0], [1088.0, 576.0], 1.0);
        near(v.center, [0.0, 0.0]);
        assert!(v.zoom > 0.0 && v.zoom <= 2.0);
    }

    #[test]
    fn fitting_a_level_uses_original_eighty_point_margin() {
        let mut view = Viewport::default();
        view.fit([0.0, 0.0, 320.0, 240.0], [800.0, 600.0], 1.0);
        assert!((view.zoom - 1.875).abs() < 0.0001);
    }

    #[test]
    fn animated_world_entry_reanchors_the_level_focus_to_the_backing_pixel() {
        let mut view = Viewport {
            center: [288., 80.],
            zoom: 900. / 336.,
        };
        let mut motion = CameraMotion::default();
        motion.enter_world([160., -48., 896., 256.], false);
        let canvas = Rect::from_min_size(
            eframe::egui::pos2(300., 0.),
            eframe::egui::vec2(1140., 900.),
        );
        motion.tick(&mut view, canvas, [160., -48.], 1., 1, 0.05);
        // The original projects the center at page pixel (869,449) before
        // zooming and at (870,450) afterward.
        near(view.center, [289.21667, 81.21667]);
    }

    #[test]
    fn idle_world_entry_uses_the_pending_canvas_interval_after_an_early_key_frame() {
        let ctx = eframe::egui::Context::default();
        let mut motion = CameraMotion::default();
        let mut view = Viewport {
            center: [239.5, 15.5],
            zoom: 900. / 207.,
        };
        let canvas = Rect::from_min_size(
            eframe::egui::pos2(300., 0.),
            eframe::egui::vec2(1140., 900.),
        );
        let run = |motion: &mut CameraMotion, now| {
            let mut dt = None;
            let _ = ctx.run(
                eframe::egui::RawInput {
                    time: Some(now),
                    ..Default::default()
                },
                |ctx| dt = motion.window_dt(ctx, false),
            );
            dt
        };
        assert!(run(&mut motion, 0.).is_some());
        assert!(run(&mut motion, 7.).is_some());
        motion.enter_world([-281., -48., 319., 426.], false);
        assert_eq!(run(&mut motion, 7.015), None);
        let elapsed = run(&mut motion, 7.085).unwrap();
        assert!((elapsed - 0.085).abs() < 0.0001);
        assert!(!motion.tick(&mut view, canvas, [160., -48.], 1., 1, elapsed));
        assert!((view.zoom - 0.95).abs() < 0.0001);
        near(view.center, [239.5, 15.5]);
    }

    #[test]
    fn world_entry_uses_backing_zoom_floor_before_halving_and_only_ticks_once_per_frame() {
        let mut view = Viewport {
            zoom: 0.1,
            ..Viewport::default()
        };
        let mut motion = CameraMotion::default();
        motion.enter_world([0.0, 0.0, 100_000.0, 100_000.0], true);
        let canvas = Rect::from_min_size(eframe::egui::Pos2::ZERO, eframe::egui::vec2(350., 300.));
        assert!(motion.tick(&mut view, canvas, [0.; 2], 2.0, 1, 1.0 / 30.0));
        assert!((view.zoom - 0.095).abs() < 0.0001);
        assert!(motion.tick(&mut view, canvas, [0.; 2], 2.0, 1, 1.0 / 60.0));
        assert!((view.zoom - 0.095).abs() < 0.0001);
        assert!(motion.tick(&mut view, canvas, [0.; 2], 2.0, 2, 0.5));
        assert!((view.zoom - 0.091).abs() < 0.0001);
    }

    #[test]
    fn camera_fit_finishes_pan_and_zoom_with_subpixel_steps() {
        let bounds = [0., 0., 320., 240.];
        let canvas = Rect::from_min_size(
            eframe::egui::pos2(300., 0.),
            eframe::egui::vec2(1140., 900.),
        );
        for density in [1., 2.] {
            let mut view = Viewport {
                center: [440., 550.],
                zoom: 1.4,
            };
            let mut motion = CameraMotion::default();
            motion.fit(bounds, false);
            let center = fit_center(bounds);
            let zoom = fit_zoom(bounds, canvas.size().into(), density, false);
            let (mut last_pan, mut last_zoom) = (None, None);
            for frame in 1..=120 {
                let previous = view.clone();
                let active = motion.tick(&mut view, canvas, [0.; 2], density, frame, 1. / 60.);
                if view.center == center && previous.center != center {
                    let dx = view.center[0] - previous.center[0];
                    let dy = view.center[1] - previous.center[1];
                    last_pan = Some(dx.hypot(dy) * view.zoom * density);
                }
                if view.zoom == zoom && previous.zoom != zoom {
                    last_zoom = Some(
                        (view.zoom / previous.zoom - 1.).abs() * canvas.width() * 0.5 * density,
                    );
                }
                if !active {
                    break;
                }
            }
            assert!(!motion.is_active());
            assert_eq!(view.center, center);
            assert_eq!(view.zoom, zoom);
            let (pan, zoom) = (last_pan.unwrap(), last_zoom.unwrap());
            assert!(
                pan < 0.51 && zoom < 0.51,
                "at {density}x, final pan and zoom moved {pan} and {zoom} physical pixels"
            );
        }
    }

    #[test]
    fn camera_fit_settles_at_the_zoom_limit() {
        let mut view = Viewport::default();
        let mut motion = CameraMotion::default();
        motion.fit([0.0, 0.0, 16.0, 16.0], false);
        let canvas = Rect::from_min_size(
            eframe::egui::Pos2::ZERO,
            eframe::egui::vec2(10_000., 10_000.),
        );
        for frame in 1..54 {
            if !motion.tick(&mut view, canvas, [0.; 2], 2.0, frame, 1.0 / 30.0) {
                break;
            }
        }
        assert!(!motion.is_active());
        assert_eq!(view.center, [8.0, 8.0]);
        assert_eq!(view.zoom, 16.0);
    }

    #[test]
    fn a_fast_brush_drag_does_not_leave_holes() {
        assert_eq!(
            stroke_cells([0, 0], [4, 2]),
            vec![[0, 0], [1, 1], [2, 1], [3, 2], [4, 2]]
        );
        assert_eq!(stroke_cells([2, 0], [0, 0]), vec![[2, 0], [1, 0], [0, 0]]);
        assert_eq!(stroke_cells([1, 1], [1, 1]), vec![[1, 1]]);
    }

    #[test]
    fn flips_reverse_uv_axes_without_moving_tile() {
        assert_eq!(
            tile_uv([16.0, 32.0], [16.0, 16.0], [64.0, 64.0], 0),
            [[0.25, 0.5], [0.5, 0.75]]
        );
        assert_eq!(
            tile_uv([16.0, 32.0], [16.0, 16.0], [64.0, 64.0], 3),
            [[0.5, 0.75], [0.25, 0.5]]
        );
    }
}
