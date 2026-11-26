use eframe::egui::{Rect, vec2};
#[derive(Debug, Clone, PartialEq)]
pub(super) struct TileQuad {
    pub dst: Rect,
    pub src: Rect,
}
pub(super) fn entity_tiles(
    bounds: Rect,
    source: Rect,
    pivot: [f32; 2],
    mode: &str,
    borders: [f32; 4],
) -> Vec<TileQuad> {
    entity_tiles_clipped(bounds, source, pivot, mode, borders, Rect::EVERYTHING)
}
pub(super) fn entity_tiles_clipped(
    bounds: Rect,
    source: Rect,
    pivot: [f32; 2],
    mode: &str,
    borders: [f32; 4],
    clip: Rect,
) -> Vec<TileQuad> {
    if source.width() <= 0.
        || source.height() <= 0.
        || bounds.width() <= 0.
        || bounds.height() <= 0.
    {
        return vec![];
    }
    let mut out = Vec::new();
    let anchor = bounds.min + bounds.size() * vec2(pivot[0], pivot[1]);
    match mode {
        "Repeat" => repeat(bounds, source, clip, &mut out),
        "NineSlice" => {
            // Borders follow LDtk's top,right,bottom,left order. Heaps ScaleGrid
            // repeats edges and center, retaining unscaled border pixel widths.
            let [top, right, bottom, left] = borders;
            let left = left.clamp(0., source.width());
            let right = right.clamp(0., source.width() - left);
            let top = top.clamp(0., source.height());
            let bottom = bottom.clamp(0., source.height() - top);
            let sx = [
                source.left(),
                source.left() + left,
                source.right() - right,
                source.right(),
            ];
            let sy = [
                source.top(),
                source.top() + top,
                source.bottom() - bottom,
                source.bottom(),
            ];
            let fx = (bounds.width() / (left + right).max(0.001)).min(1.);
            let fy = (bounds.height() / (top + bottom).max(0.001)).min(1.);
            let dx = [
                bounds.left(),
                bounds.left() + left * fx,
                bounds.right() - right * fx,
                bounds.right(),
            ];
            let dy = [
                bounds.top(),
                bounds.top() + top * fy,
                bounds.bottom() - bottom * fy,
                bounds.bottom(),
            ];
            for y in 0..3 {
                for x in 0..3 {
                    let src =
                        Rect::from_min_max([sx[x], sy[y]].into(), [sx[x + 1], sy[y + 1]].into());
                    let dst =
                        Rect::from_min_max([dx[x], dy[y]].into(), [dx[x + 1], dy[y + 1]].into());
                    if x == 1 || y == 1 {
                        repeat(dst, src, clip, &mut out)
                    } else if dst.intersects(clip) && dst.is_positive() {
                        out.push(TileQuad { dst, src })
                    }
                }
            }
        }
        "FitInside" => {
            let scale = (bounds.width() / source.width()).min(bounds.height() / source.height());
            let size = source.size() * scale;
            out.push(TileQuad {
                dst: Rect::from_min_size(anchor - size * vec2(pivot[0], pivot[1]), size),
                src: source,
            });
        }
        "Cover" | "FullSizeCropped" => {
            let scale = if mode == "Cover" {
                (bounds.width() / source.width()).max(bounds.height() / source.height())
            } else {
                1.
            };
            let size = bounds.size().min(source.size() * scale);
            let crop = size / scale;
            let src = Rect::from_min_size(
                source.min + (source.size() - crop) * vec2(pivot[0], pivot[1]),
                crop,
            );
            out.push(TileQuad {
                dst: Rect::from_min_size(anchor - size * vec2(pivot[0], pivot[1]), size),
                src,
            });
        }
        "FullSizeUncropped" => out.push(TileQuad {
            dst: Rect::from_min_size(
                anchor - source.size() * vec2(pivot[0], pivot[1]),
                source.size(),
            ),
            src: source,
        }),
        _ => out.push(TileQuad {
            dst: bounds,
            src: source,
        }),
    }
    out.retain(|quad| quad.dst.intersects(clip));
    out
}
fn repeat(dst: Rect, src: Rect, clip: Rect, out: &mut Vec<TileQuad>) {
    if !dst.is_positive() || !src.is_positive() || !dst.intersects(clip) {
        return;
    }
    let visible = dst.intersect(clip);
    let x0 = ((visible.left() - dst.left()) / src.width())
        .floor()
        .max(0.) as i32;
    let y0 = ((visible.top() - dst.top()) / src.height()).floor().max(0.) as i32;
    let x1 = ((visible.right() - dst.left()) / src.width()).ceil() as i32;
    let y1 = ((visible.bottom() - dst.top()) / src.height()).ceil() as i32;
    for y in y0..y1 {
        for x in x0..x1 {
            let min = dst.min + vec2(x as f32 * src.width(), y as f32 * src.height());
            let size = src.size().min(dst.max - min);
            out.push(TileQuad {
                dst: Rect::from_min_size(min, size),
                src: Rect::from_min_size(src.min, size),
            });
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn rect(x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect::from_min_size([x, y].into(), vec2(w, h))
    }
    #[test]
    fn fit_inside_and_cover_use_entity_pivot() {
        let b = rect(100., 200., 40., 40.);
        let s = rect(16., 32., 32., 16.);
        assert_eq!(
            entity_tiles(b, s, [0.5, 1.], "FitInside", [0.; 4]),
            vec![TileQuad {
                dst: rect(100., 220., 40., 20.),
                src: s
            }]
        );
        assert_eq!(
            entity_tiles(b, s, [0.5, 1.], "Cover", [0.; 4]),
            vec![TileQuad {
                dst: b,
                src: rect(24., 32., 16., 16.)
            }]
        );
    }
    #[test]
    fn repeat_crops_partial_tiles_without_stretching() {
        let q = entity_tiles(
            rect(0., 0., 20., 10.),
            rect(16., 16., 8., 8.),
            [0.; 2],
            "Repeat",
            [0.; 4],
        );
        assert_eq!(q.len(), 6);
        assert_eq!(
            q[5],
            TileQuad {
                dst: rect(16., 8., 4., 2.),
                src: rect(16., 16., 4., 2.)
            }
        );
    }
    #[test]
    fn nine_slice_keeps_corners_and_repeats_center() {
        let q = entity_tiles(
            rect(0., 0., 30., 20.),
            rect(40., 60., 12., 12.),
            [0.; 2],
            "NineSlice",
            [2., 3., 4., 5.],
        );
        assert!(q.contains(&TileQuad {
            dst: rect(0., 0., 5., 2.),
            src: rect(40., 60., 5., 2.)
        }));
        assert!(q.contains(&TileQuad {
            dst: rect(27., 16., 3., 4.),
            src: rect(49., 68., 3., 4.)
        }));
        assert!(q.iter().all(|q| q.dst.max.x <= 30. && q.dst.max.y <= 20.));
    }
}

pub(super) struct LayerTileOffset {
    grid: [f32; 2],
    tile: Option<[f32; 2]>,
}

impl LayerTileOffset {
    pub(super) fn new(
        grid: f32,
        tile_size: f32,
        pivot: [f32; 2],
        flips: i64,
        is_auto: bool,
    ) -> Self {
        let flipped = |axis| flips & (1_i64 << axis) != 0;
        Self {
            grid: [0, 1].map(|axis| {
                let pivot = if is_auto { pivot[axis] } else { pivot[0] };
                pivot * grid + if flipped(axis) { grid - tile_size } else { 0. }
            }),
            tile: (!is_auto).then(|| {
                [0, 1].map(|axis| {
                    let pivot = if flipped(axis) {
                        pivot[axis]
                    } else {
                        -pivot[axis]
                    };
                    pivot * tile_size
                })
            }),
        }
    }

    pub(super) fn apply(&self, axis: usize, anchor: f32) -> f32 {
        // Keep the two additions separate so pixels round as in the layer renderer.
        let mut position = anchor + self.grid[axis];
        if let Some(tile) = self.tile {
            position += tile[axis];
        }
        position
    }
}
#[cfg(test)]
mod upstream_tests {
    use super::*;

    fn layer_tile_rect(
        px: [f32; 2],
        grid: f32,
        tile_size: f32,
        pivot: [f32; 2],
        flips: i64,
        is_auto: bool,
    ) -> Rect {
        let offset = LayerTileOffset::new(grid, tile_size, pivot, flips, is_auto);
        let min = [0, 1].map(|axis| offset.apply(axis, px[axis]));
        Rect::from_min_size(min.into(), vec2(tile_size, tile_size))
    }
    #[test]
    fn full_size_modes_keep_pivot_and_unscaled_dimensions() {
        let b = Rect::from_min_size([20., 30.].into(), vec2(12., 8.));
        let s = Rect::from_min_size([50., 60.].into(), vec2(20., 16.));
        assert_eq!(
            entity_tiles(b, s, [1., 0.5], "FullSizeUncropped", [0.; 4])[0].dst,
            Rect::from_min_size([12., 26.].into(), vec2(20., 16.))
        );
        assert_eq!(
            entity_tiles(b, s, [1., 0.5], "FullSizeCropped", [0.; 4])[0],
            TileQuad {
                dst: b,
                src: Rect::from_min_size([58., 64.].into(), vec2(12., 8.))
            }
        );
        assert_eq!(
            entity_tiles(b, s, [1., 0.5], "Stretch", [0.; 4])[0],
            TileQuad { dst: b, src: s }
        );
    }
    #[test]
    fn grid_tiles_reproduce_original_asymmetric_y_pivot() {
        assert_eq!(
            layer_tile_rect([0., 0.], 16., 32., [0.25, 0.75], 0, false),
            Rect::from_min_size([-4., -20.].into(), vec2(32., 32.))
        );
    }
    #[test]
    fn auto_tiles_flip_around_cell_and_keep_layer_pivot() {
        assert_eq!(
            layer_tile_rect([32., 48.], 16., 32., [0.25, 0.75], 3, true),
            Rect::from_min_size([20., 44.].into(), vec2(32., 32.))
        );
    }
}

pub(super) fn aligned_repeat(
    bounds: Rect,
    source: Rect,
    pivot: [f32; 2],
    clip: Rect,
) -> Vec<TileQuad> {
    if !source.is_positive() {
        return vec![];
    }
    let initial = (bounds.size() - source.size()) * vec2(pivot[0], pivot[1]);
    let initial = initial
        - vec2(
            (initial.x / source.width()).ceil() * source.width(),
            (initial.y / source.height()).ceil() * source.height(),
        );
    let dst = Rect::from_min_max(bounds.min + initial, bounds.max);
    let mut out = Vec::new();
    repeat(dst, source, bounds.intersect(clip), &mut out);
    out
}
#[cfg(test)]
mod repeat_tests {
    use super::*;
    #[test]
    fn background_repeat_aligns_texture_anchor_not_remaining_width() {
        let b = Rect::from_min_size([0., 0.].into(), vec2(20., 20.));
        let s = Rect::from_min_size([0., 0.].into(), vec2(8., 8.));
        let q = aligned_repeat(b, s, [0.5, 0.5], b);
        assert_eq!(q[0].dst.min, Pos2::new(-2., -2.));
        assert_eq!(q.last().unwrap().dst.max, b.max);
        let anchor_region = Rect::from_min_size(b.min, s.size());
        let anchor = aligned_repeat(b, s, [0.5, 0.5], anchor_region);
        assert_eq!(anchor.first(), q.first());
    }
    use eframe::egui::Pos2;
}
