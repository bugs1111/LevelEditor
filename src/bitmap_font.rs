//! Original LDtk bitmap glyphs; XML advances/bearings are used without a system
//! font substitution, so labels retain their original metrics at each DPI.
use crate::assets::Assets;
use eframe::egui::{self, Color32, Mesh, Painter, Pos2, Rect, Vec2, vec2};
use std::{collections::HashMap, sync::OnceLock};
#[derive(Debug, Clone, Copy)]
struct Glyph {
    advance: f32,
    offset: Vec2,
    source: Rect,
}
pub(super) struct BitmapFont {
    height: f32,
    baseline: f32,
    glyphs: HashMap<char, Glyph>,
    name: &'static str,
    png: &'static [u8],
}
fn attr<'a>(line: &'a str, name: &str) -> Option<&'a str> {
    let start = line.find(&format!("{name}=\""))? + name.len() + 2;
    line[start..].split('"').next()
}
fn numbers(text: &str) -> Vec<f32> {
    text.split_whitespace()
        .filter_map(|x| x.parse().ok())
        .collect()
}
fn decode(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(start) = rest.find('&') {
        out.push_str(&rest[..start]);
        let Some(end) = rest[start..].find(';') else {
            out.push_str(&rest[start..]);
            return out;
        };
        let entity = &rest[start + 1..start + end];
        let decoded = match entity {
            "quot" => Some('"'),
            "apos" => Some('\''),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "amp" => Some('&'),
            _ => entity
                .strip_prefix("#x")
                .and_then(|s| u32::from_str_radix(s, 16).ok())
                .or_else(|| entity.strip_prefix('#').and_then(|s| s.parse().ok()))
                .and_then(char::from_u32),
        };
        if let Some(c) = decoded {
            out.push(c)
        } else {
            out.push_str(&rest[start..=start + end])
        }
        rest = &rest[start + end + 1..];
    }
    out.push_str(rest);
    out
}
impl BitmapFont {
    fn parse(xml: &str, name: &'static str, png: &'static [u8]) -> Self {
        let header = xml.lines().find(|l| l.starts_with("<Font")).unwrap_or("");
        let height = attr(header, "height")
            .and_then(|s| s.parse().ok())
            .unwrap_or(13.);
        let mut glyphs = HashMap::new();
        for line in xml.lines().filter(|l| l.trim_start().starts_with("<Char ")) {
            let Some(code) = attr(line, "code")
                .map(decode)
                .and_then(|s| s.chars().next())
            else {
                continue;
            };
            let rect = numbers(attr(line, "rect").unwrap_or(""));
            let offset = numbers(attr(line, "offset").unwrap_or(""));
            if rect.len() != 4 || offset.len() != 2 {
                continue;
            }
            glyphs.insert(
                code,
                Glyph {
                    advance: attr(line, "width")
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(rect[2]),
                    offset: vec2(offset[0], offset[1]),
                    source: Rect::from_min_size([rect[0], rect[1]].into(), vec2(rect[2], rect[3])),
                },
            );
        }
        let padding = glyphs.get(&' ').map_or(0., |g| g.source.height() * 0.5);
        let baseline = ['A', 'a', '0']
            .iter()
            .find_map(|c| glyphs.get(c))
            .map_or(height - 2. - padding, |g| {
                g.offset.y + g.source.height() - padding
            });
        Self {
            height,
            baseline,
            glyphs,
            name,
            png,
        }
    }
    // Heaps Text.splitRawText: break at spaces/CJK, never in a Latin word.
    // The break space is discarded; CJK punctuation stays with the prior glyph.
    fn layout(&self, text: &str, max_width: f32) -> (Vec2, Vec<(Vec2, Glyph)>) {
        let chars = text.chars().filter(|c| *c != '\r').collect::<Vec<_>>();
        let glyph = |c| {
            self.glyphs
                .get(&c)
                .or_else(|| self.glyphs.get(&'?'))
                .copied()
        };
        let advance = |c| {
            if c == '\n' {
                0.
            } else {
                glyph(c).map_or(0., |g| g.advance)
            }
        };
        let space = |c| matches!(c, ' ' | '\u{3000}');
        let breaker =
            |c| space(c) || matches!(c as u32,11904..=42191|63744..=64255|131072..=262141);
        let complement = |c| {
            "ヽヾーァィゥェォッャュョヮヵヶぁぃぅぇぉっゃゅょゎゕゖㇰㇱㇲㇳㇴㇵㇶㇷㇸㇹㇺㇻㇼㇽㇾㇿ々〻。，".contains(c)
        };
        let mut pen = Vec2::ZERO;
        let mut width = 0f32;
        let mut placed = Vec::with_capacity(chars.len());
        for (i, &ch) in chars.iter().enumerate() {
            let mut newline = ch == '\n';
            let mut skip = newline;
            if breaker(ch) && chars.get(i + 1).is_none_or(|c| !complement(*c)) {
                let mut size = pen.x + advance(ch);
                let mut k = i + 1;
                while size <= max_width && k < chars.len() {
                    let next = chars[k];
                    k += 1;
                    if space(next) || next == '\n' {
                        break;
                    }
                    size += advance(next);
                    // The upstream look-ahead uses k+1 after advancing k.
                    if breaker(next) && chars.get(k + 1).is_none_or(|c| !complement(*c)) {
                        break;
                    }
                }
                if size > max_width {
                    newline = true;
                    skip = space(ch);
                }
            }
            if !skip && let Some(g) = glyph(ch) {
                if g.source.is_positive() {
                    placed.push((pen + g.offset, g));
                }
                pen.x += g.advance;
            }
            if newline {
                width = width.max(pen.x);
                pen.x = 0.;
                pen.y += self.height;
            }
        }
        (
            vec2(
                width.max(pen.x),
                if text.is_empty() {
                    0.
                } else {
                    pen.y + self.height
                },
            ),
            placed,
        )
    }
}
pub(super) fn regular(ctx: &egui::Context) -> (&'static BitmapFont, f32) {
    static TINY: OnceLock<BitmapFont> = OnceLock::new();
    static REGULAR: OnceLock<BitmapFont> = OnceLock::new();
    let dpi = ctx.pixels_per_point();
    if dpi <= 1. {
        (
            TINY.get_or_init(|| {
                BitmapFont::parse(
                    include_str!(
                        "../assets/bitmap_fonts/noto_sans_display_semicondensed_medium_12.xml"
                    ),
                    "font-12",
                    include_bytes!(
                        "../assets/bitmap_fonts/noto_sans_display_semicondensed_medium_12.png"
                    ),
                )
            }),
            1.,
        )
    } else {
        (
            REGULAR.get_or_init(|| {
                BitmapFont::parse(
                    include_str!(
                        "../assets/bitmap_fonts/noto_sans_display_semicondensed_medium_19.xml"
                    ),
                    "font-19",
                    include_bytes!(
                        "../assets/bitmap_fonts/noto_sans_display_semicondensed_medium_19.png"
                    ),
                )
            }),
            1. / dpi,
        )
    }
}
pub(super) fn measure(ctx: &egui::Context, text: &str, scale: f32, max_width: f32) -> Vec2 {
    let (font, dpi_scale) = regular(ctx);
    let scale = scale * dpi_scale;
    font.layout(text, max_width / scale.max(0.001)).0 * scale
}
// h2d.Text.getBoundsRec(forSize=true) uses the baseline for its last line.
pub(super) fn measure_flow(ctx: &egui::Context, text: &str, scale: f32, max_width: f32) -> Vec2 {
    let (font, dpi_scale) = regular(ctx);
    let scale = scale * dpi_scale;
    let mut size = font.layout(text, max_width / scale.max(0.001)).0;
    if !text.is_empty() {
        size.y += font.baseline - font.height;
    }
    size * scale
}
#[allow(clippy::too_many_arguments)]
pub(super) fn paint(
    ctx: &egui::Context,
    painter: &Painter,
    assets: &mut Assets,
    pos: Pos2,
    text: &str,
    color: Color32,
    scale: f32,
    max_width: f32,
    outline: bool,
) -> Vec2 {
    paint_rotated(
        ctx, painter, assets, pos, text, color, scale, max_width, outline, 0.,
    )
}
#[allow(clippy::too_many_arguments)]
pub(super) fn paint_rotated(
    ctx: &egui::Context,
    painter: &Painter,
    assets: &mut Assets,
    pos: Pos2,
    text: &str,
    color: Color32,
    scale: f32,
    max_width: f32,
    outline: bool,
    angle: f32,
) -> Vec2 {
    let (font, dpi_scale) = regular(ctx);
    paint_font(
        ctx,
        painter,
        assets,
        pos,
        text,
        color,
        scale * dpi_scale,
        max_width,
        outline.then_some(color),
        angle,
        font,
    )
}
#[allow(clippy::too_many_arguments)]
pub(super) fn paint_field(
    ctx: &egui::Context,
    painter: &Painter,
    assets: &mut Assets,
    pos: Pos2,
    text: &str,
    color: Color32,
    scale: f32,
    max_width: f32,
    outline: Option<Color32>,
) -> Vec2 {
    let (font, dpi_scale) = regular(ctx);
    paint_font(
        ctx,
        painter,
        assets,
        pos,
        text,
        color,
        scale * dpi_scale,
        max_width,
        outline,
        0.,
        font,
    )
}

fn title_font() -> &'static BitmapFont {
    static TITLE: OnceLock<BitmapFont> = OnceLock::new();
    TITLE.get_or_init(|| {
        BitmapFont::parse(
            include_str!(
                "../assets/bitmap_fonts/noto_sans_display_semicondensed_extralight_90.xml"
            ),
            "font-title-90",
            include_bytes!(
                "../assets/bitmap_fonts/noto_sans_display_semicondensed_extralight_90.png"
            ),
        )
    })
}
pub(super) fn title_measure(text: &str) -> Vec2 {
    title_font().layout(text, f32::INFINITY).0
}
pub(super) fn title_paint(
    ctx: &egui::Context,
    painter: &Painter,
    assets: &mut Assets,
    pos: Pos2,
    text: &str,
    color: Color32,
    scale: f32,
) {
    paint_font(
        ctx,
        painter,
        assets,
        pos,
        text,
        color,
        scale,
        f32::INFINITY,
        None,
        0.,
        title_font(),
    );
}
#[allow(clippy::too_many_arguments)]
fn paint_font(
    ctx: &egui::Context,
    painter: &Painter,
    assets: &mut Assets,
    pos: Pos2,
    text: &str,
    color: Color32,
    scale: f32,
    max_width: f32,
    outline: Option<Color32>,
    angle: f32,
    font: &BitmapFont,
) -> Vec2 {
    let (size, glyphs) = font.layout(text, max_width / scale.max(0.001));
    let Some(texture) = assets.builtin(ctx, font.name, font.png) else {
        return size * scale;
    };
    let mut mesh = Mesh::with_texture(texture.id());
    let offsets: &[[f32; 2]] = if outline.is_some() {
        &[
            [-1., -1.],
            [0., -1.],
            [1., -1.],
            [-1., 0.],
            [1., 0.],
            [-1., 1.],
            [0., 1.],
            [1., 1.],
            [0., 0.],
        ]
    } else {
        &[[0., 0.]]
    };
    let (sin, cos) = angle.sin_cos();
    for (pass, off) in offsets.iter().enumerate() {
        let tint = if pass + 1 == offsets.len() {
            color
        } else {
            Color32::from_rgba_unmultiplied(
                (outline.unwrap_or(color).r() as f32 * 0.34) as u8,
                (outline.unwrap_or(color).g() as f32 * 0.34) as u8,
                (outline.unwrap_or(color).b() as f32 * 0.34) as u8,
                (outline.unwrap_or(color).a() as f32 * 0.6) as u8,
            )
        };
        for (offset, glyph) in &glyphs {
            let min = *offset * scale + vec2(off[0], off[1]);
            let max = min + glyph.source.size() * scale;
            let base = mesh.vertices.len() as u32;
            let corners = [min, vec2(max.x, min.y), max, vec2(min.x, max.y)];
            let uv = Rect::from_min_max(
                (glyph.source.min.to_vec2() / texture.size_vec2()).to_pos2(),
                (glyph.source.max.to_vec2() / texture.size_vec2()).to_pos2(),
            );
            for (corner, uv) in corners.into_iter().zip([
                uv.left_top(),
                uv.right_top(),
                uv.right_bottom(),
                uv.left_bottom(),
            ]) {
                mesh.vertices.push(egui::epaint::Vertex {
                    pos: pos
                        + vec2(
                            corner.x * cos - corner.y * sin,
                            corner.x * sin + corner.y * cos,
                        ),
                    uv,
                    color: tint,
                });
            }
            mesh.indices
                .extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
        }
    }
    painter.add(egui::Shape::mesh(mesh));
    size * scale
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_xml_metrics_and_entities_are_preserved() {
        let f = BitmapFont::parse(
            include_str!("../assets/bitmap_fonts/noto_sans_display_semicondensed_medium_12.xml"),
            "test",
            &[],
        );
        let a = f.glyphs[&'A'];
        assert_eq!(a.advance, 7.);
        assert_eq!(a.offset, vec2(0., 4.));
        assert_eq!(
            a.source,
            Rect::from_min_size([208., 2.].into(), vec2(7., 9.))
        );
        assert!(f.glyphs.contains_key(&'&'));
        assert_eq!(f.layout("A A\nAA", 100.).0, vec2(17., 26.));
    }
    #[test]
    fn heaps_wraps_at_words_and_allows_unbroken_words_to_overflow() {
        let f = BitmapFont::parse(
            include_str!("../assets/bitmap_fonts/noto_sans_display_semicondensed_medium_12.xml"),
            "test",
            &[],
        );
        let (size, glyphs) = f.layout("AAAA", 14.);
        assert_eq!(size, vec2(28., 13.));
        assert_eq!(f.layout("AA AA", 20.).0, vec2(14., 26.));
        assert_eq!(glyphs.len(), 4);
    }
}
