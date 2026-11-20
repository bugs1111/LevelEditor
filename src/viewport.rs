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
    pub fn zoom_to_at(&mut self, zoom: f32, cursor: [f32; 2], origin: [f32; 2], size: [f32; 2]) {
        let before = self.to_world(cursor, origin, size);
        self.zoom = zoom;
        let after = self.to_world(cursor, origin, size);
        self.center[0] += before[0] - after[0];
        self.center[1] += before[1] - after[1];
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

#[derive(Default)]
pub struct CameraMotion {
    zoom_request: Option<ZoomRequest>,
    zoom: Option<(f32, f32)>,
    center: Option<([f32; 2], f32)>,
    last_frame: Option<u64>,
    last_tmod: Option<f32>,
}

impl CameraMotion {
    pub fn fit(&mut self, bounds: [f32; 4], world: bool) {
        self.scroll_to(fit_center(bounds));
        self.zoom_request = Some(ZoomRequest::Fit { bounds, world });
    }

    pub fn scroll_to(&mut self, center: [f32; 2]) {
        self.center = Some((center, CAMERA_MOTION_FRAMES));
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
        self.center = Some((fit_center(level_bounds), CAMERA_MOTION_FRAMES));
        self.zoom_request = Some(ZoomRequest::LevelEntry {
            level_bounds,
            world_bounds,
            level_origin,
        });
    }

    pub fn translate_center(&mut self, offset: [f32; 2]) {
        if let Some((center, _)) = &mut self.center {
            center[0] += offset[0];
            center[1] += offset[1];
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
        Some(if dt < 0.5 {
            dt * CAMERA_FPS
        } else {
            self.last_tmod.unwrap_or(1.0)
        })
    }

    pub fn tick(
        &mut self,
        view: &mut Viewport,
        size: [f32; 2],
        pixels_per_point: f32,
        frame: u64,
        dt: f32,
    ) -> bool {
        let Some(tmod) = self.frame_tmod(frame, dt) else {
            return self.is_active();
        };
        self.last_frame = Some(frame);
        self.last_tmod = Some(tmod);
        if let Some(request) = self.zoom_request.take() {
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
            self.zoom = Some((target, CAMERA_MOTION_FRAMES));
        }
        if let Some((target, remaining)) = &mut self.zoom {
            let step = (0.22 * view.zoom * pixels_per_point).max(0.1) * tmod;
            view.zoom = (view.zoom + (*target - view.zoom) * step.min(1.0))
                .clamp(0.03 / pixels_per_point, 32.0 / pixels_per_point);
            *remaining -= tmod;
            if (*target - view.zoom).abs() <= 0.04 * view.zoom || *remaining <= 0.0 {
                self.zoom = None;
            }
        }
        if let Some((target, remaining)) = &mut self.center {
            let step = (0.15 * tmod).min(1.0);
            view.center[0] += (target[0] - view.center[0]) * step;
            view.center[1] += (target[1] - view.center[1]) * step;
            *remaining -= tmod;
            if (target[0] - view.center[0]).hypot(target[1] - view.center[1]) <= 6.0
                || *remaining <= 0.0
            {
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
    fn zoom_keeps_cursor_world_point_stationary() {
        let mut v = Viewport {
            center: [100.0, 200.0],
            zoom: 2.0,
        };
        v.zoom_to_at(4.0, [800.0, 400.0], [300.0, 0.0], [800.0, 600.0]);
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
    fn world_entry_uses_backing_zoom_floor_before_halving_and_only_ticks_once_per_frame() {
        let mut view = Viewport {
            zoom: 0.1,
            ..Viewport::default()
        };
        let mut motion = CameraMotion::default();
        motion.enter_world([0.0, 0.0, 100_000.0, 100_000.0], true);
        let size = [350.0, 300.0];
        assert!(motion.tick(&mut view, size, 2.0, 1, 1.0 / 30.0));
        assert!((view.zoom - 0.095).abs() < 0.0001);
        assert!(motion.tick(&mut view, size, 2.0, 1, 1.0 / 60.0));
        assert!((view.zoom - 0.095).abs() < 0.0001);
        assert!(motion.tick(&mut view, size, 2.0, 2, 0.5));
        assert!((view.zoom - 0.091).abs() < 0.0001);
    }

    #[test]
    fn camera_motion_times_out_even_when_the_fit_target_is_beyond_the_zoom_limit() {
        let mut view = Viewport::default();
        let mut motion = CameraMotion::default();
        motion.fit([0.0, 0.0, 16.0, 16.0], false);
        for frame in 1..=54 {
            assert_eq!(
                motion.tick(&mut view, [10_000.0, 10_000.0], 2.0, frame, 1.0 / 30.0),
                frame < 54,
            );
            assert_eq!(view.zoom, 16.0);
        }
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
