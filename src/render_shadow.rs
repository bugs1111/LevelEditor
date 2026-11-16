//! LevelRender's black Glow(0, .6, 128, smoothColor=true), knockout=true.
//! A separable convolution of a filled rectangle is represented by a bounded
//! nine-slice texture; it does not require a full level-sized render target.
use eframe::egui::{self, Color32, ColorImage, Painter, Rect, TextureHandle, TextureOptions};
const EDGE: usize = 40;
fn pixels(width: usize, height: usize) -> ColorImage {
    let taps = super::blur::kernel(128.);
    let coverage = |pixel: usize, extent: usize| {
        let x = pixel as f32 - EDGE as f32 + 0.5;
        let sample = |x: f32| (x + 0.5).clamp(0., 1.) - (x - extent as f32 + 0.5).clamp(0., 1.);
        let mut alpha = sample(x) * taps[0][1];
        for &[offset, weight] in &taps[1..] {
            alpha += (sample(x - offset) + sample(x + offset)) * weight;
        }
        alpha
    };
    let w = width + EDGE * 2;
    let h = height + EDGE * 2;
    let xs = (0..w)
        .map(|x| (coverage(x, width) * 0.9 * 255.).round() / 255.)
        .collect::<Vec<_>>();
    let ys = (0..h).map(|y| coverage(y, height)).collect::<Vec<_>>();
    let mut image = ColorImage::filled([w, h], Color32::TRANSPARENT);
    for (y, &cy) in ys.iter().enumerate() {
        for (x, &cx) in xs.iter().enumerate() {
            if (EDGE..EDGE + width).contains(&x) && (EDGE..EDGE + height).contains(&y) {
                continue; // Glow.knockout erases the unblurred rectangle.
            }
            // smoothColor uses alpha*1.5 in each separable pass, including the
            // intermediate RGBA8 quantization of the original render target.
            image.pixels[x + y * w] =
                Color32::from_black_alpha((cx * cy * 0.9 * 255.).round() as u8);
        }
    }
    image
}
pub(super) fn paint(painter: &Painter, rect: Rect, zoom: f32) {
    let full = [
        (rect.width() / zoom).round().max(1.) as usize,
        (rect.height() / zoom).round().max(1.) as usize,
    ];
    let capped = full.map(|n| n.min(EDGE * 2 + 1));
    let id = egui::Id::new("ldtk-level-bounds-shadow");
    let cached = painter
        .ctx()
        .data(|d| d.get_temp::<([usize; 2], TextureHandle)>(id));
    let texture = if let Some((_, texture)) = cached.filter(|(size, _)| *size == capped) {
        texture
    } else {
        let texture = painter.ctx().load_texture(
            "ldtk-level-bounds-shadow",
            pixels(capped[0], capped[1]),
            TextureOptions::NEAREST,
        );
        painter
            .ctx()
            .data_mut(|d| d.insert_temp(id, (capped, texture.clone())));
        texture
    };
    let coordinates = |axis: usize| {
        let edge = (full[axis] as f32 * 0.5).min(EDGE as f32);
        let dst = [
            -(EDGE as f32),
            edge,
            full[axis] as f32 - edge,
            full[axis] as f32 + EDGE as f32,
        ];
        let src = [
            0.,
            EDGE as f32 + edge,
            EDGE as f32 + capped[axis] as f32 - edge,
            (capped[axis] + EDGE * 2) as f32,
        ];
        (dst, src)
    };
    let (dx, sx) = coordinates(0);
    let (dy, sy) = coordinates(1);
    let size = texture.size_vec2();
    for y in 0..3 {
        for x in 0..3 {
            if x == 1 && y == 1 {
                continue;
            }
            let dst = Rect::from_min_max(
                rect.min + egui::vec2(dx[x], dy[y]) * zoom,
                rect.min + egui::vec2(dx[x + 1], dy[y + 1]) * zoom,
            );
            if dst.is_positive() && dst.intersects(painter.clip_rect()) {
                painter.image(
                    texture.id(),
                    dst,
                    Rect::from_min_max(
                        egui::pos2(sx[x] / size.x, sy[y] / size.y),
                        egui::pos2(sx[x + 1] / size.x, sy[y + 1] / size.y),
                    ),
                    Color32::WHITE,
                );
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn source_glow_kernel_and_knockout_have_nonzero_exterior_and_empty_interior() {
        let oracle: serde_json::Value =
            serde_json::from_str(include_str!("../docs/status/shadow-oracle.json")).unwrap();
        let taps = super::super::blur::kernel(128.);
        assert_eq!(taps.len(), oracle["taps"].as_array().unwrap().len());
        for (actual, expected) in taps.iter().zip(oracle["taps"].as_array().unwrap()) {
            assert!((actual[0] as f64 - expected[0].as_f64().unwrap()).abs() < 0.00001);
            assert!((actual[1] as f64 - expected[1].as_f64().unwrap()).abs() < 0.000001);
        }
        let image = pixels(81, 81);
        let at = |x: usize, y: usize| image.pixels[x + y * image.size[0]].a();
        assert_eq!(at(EDGE + 40, EDGE + 40), 0);
        assert!(at(EDGE - 1, EDGE + 40) > 90);
        assert!(at(EDGE - 8, EDGE + 40) < at(EDGE - 1, EDGE + 40));
        assert_eq!(at(0, 0), 0);
        assert_eq!(at(EDGE - 8, EDGE + 40), at(EDGE + 81 + 7, EDGE + 40));
    }
}
