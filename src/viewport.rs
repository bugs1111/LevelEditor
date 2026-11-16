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
    pub fn zoom_at(&mut self, factor: f32, cursor: [f32; 2], origin: [f32; 2], size: [f32; 2]) {
        let before = self.to_world(cursor, origin, size);
        self.zoom = (self.zoom * factor).clamp(0.03, 32.0);
        let after = self.to_world(cursor, origin, size);
        self.center[0] += before[0] - after[0];
        self.center[1] += before[1] - after[1];
    }
    pub fn fit(&mut self, bounds: [f32; 4], size: [f32; 2]) {
        self.center = [(bounds[0] + bounds[2]) * 0.5, (bounds[1] + bounds[3]) * 0.5];
        self.zoom = (((size[0] - 64.0).max(1.0) / (bounds[2] - bounds[0]).max(1.0))
            .min((size[1] - 64.0).max(1.0) / (bounds[3] - bounds[1]).max(1.0)))
        .clamp(0.03, 32.0);
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
        v.zoom_at(2.0, [800.0, 400.0], [300.0, 0.0], [800.0, 600.0]);
        near(
            v.to_world([800.0, 400.0], [300.0, 0.0], [800.0, 600.0]),
            [150.0, 250.0],
        );
        assert_eq!(v.zoom, 4.0);
    }

    #[test]
    fn fit_handles_worlds_with_negative_coordinates() {
        let mut v = Viewport::default();
        v.fit([-256.0, -128.0, 256.0, 128.0], [1088.0, 576.0]);
        near(v.center, [0.0, 0.0]);
        assert!(v.zoom > 0.0 && v.zoom <= 2.0);
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
