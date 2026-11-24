use eframe::egui::{self, Color32, FontData, FontDefinitions, FontFamily, FontId, TextStyle};
use std::sync::Arc;

pub const DARK: Color32 = Color32::from_rgb(0x1e, 0x22, 0x29);
pub const MEDIUM: Color32 = Color32::from_rgb(0x2e, 0x33, 0x3f);
pub const LIGHT: Color32 = Color32::from_rgb(0x54, 0x5d, 0x73);
pub const TEXT: Color32 = Color32::from_rgb(0xc1, 0xcf, 0xeb);
pub const ACCENT: Color32 = Color32::from_rgb(0xff, 0xcc, 0x00);
const BUNDLED_CJK: &[u8] = include_bytes!("../assets/fonts/LevelEditorCJK-Regular.otf");

pub fn configure(ctx: &egui::Context) {
    ctx.set_theme(egui::Theme::Dark);
    let mut fonts = FontDefinitions::default();
    fonts.font_data.insert(
        "ldtk-light".into(),
        Arc::new(FontData::from_static(include_bytes!(
            "../assets/fonts/RobotoCondensed-Light.ttf"
        ))),
    );
    fonts.font_data.insert(
        "ldtk-bold".into(),
        Arc::new(FontData::from_static(include_bytes!(
            "../assets/fonts/RobotoCondensed-Bold.ttf"
        ))),
    );
    fonts.font_data.insert(
        "ldtk-regular".into(),
        Arc::new(FontData::from_static(include_bytes!(
            "../assets/fonts/RobotoCondensed-Regular.ttf"
        ))),
    );
    fonts
        .families
        .entry(FontFamily::Proportional)
        .or_default()
        .insert(0, "ldtk-light".into());
    fonts
        .families
        .insert(FontFamily::Name("heading".into()), vec!["ldtk-bold".into()]);
    fonts.families.insert(
        FontFamily::Name("panel-title".into()),
        vec!["ldtk-light".into()],
    );
    fonts.families.insert(
        FontFamily::Name("panel-header".into()),
        vec!["ldtk-regular".into()],
    );
    for path in [
        "/System/Library/Fonts/Supplemental/Arial.ttf",
        "C:/Windows/Fonts/arial.ttf",
        "/usr/share/fonts/truetype/msttcorefonts/Arial.ttf",
        "/usr/share/fonts/truetype/liberation2/LiberationSans-Regular.ttf",
        "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
        "/usr/share/fonts/liberation-sans/LiberationSans-Regular.ttf",
        "/usr/share/fonts/TTF/LiberationSans-Regular.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        "/usr/share/fonts/dejavu-sans-fonts/DejaVuSans.ttf",
        "/usr/share/fonts/TTF/DejaVuSans.ttf",
    ] {
        if let Ok(bytes) = std::fs::read(path) {
            fonts
                .font_data
                .insert("system-body".into(), Arc::new(FontData::from_owned(bytes)));
            fonts
                .families
                .entry(FontFamily::Proportional)
                .or_default()
                .insert(0, "system-body".into());
            break;
        }
    }
    let fallbacks = fonts
        .families
        .get(&FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();
    let mut bold = vec!["ldtk-bold".into()];
    for path in [
        "/System/Library/Fonts/Supplemental/Arial Bold.ttf",
        "C:/Windows/Fonts/arialbd.ttf",
        "/usr/share/fonts/truetype/msttcorefonts/Arial_Bold.ttf",
        "/usr/share/fonts/truetype/liberation2/LiberationSans-Bold.ttf",
        "/usr/share/fonts/truetype/liberation/LiberationSans-Bold.ttf",
        "/usr/share/fonts/liberation-sans/LiberationSans-Bold.ttf",
        "/usr/share/fonts/TTF/LiberationSans-Bold.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf",
        "/usr/share/fonts/dejavu-sans-fonts/DejaVuSans-Bold.ttf",
        "/usr/share/fonts/TTF/DejaVuSans-Bold.ttf",
    ] {
        if let Ok(bytes) = std::fs::read(path) {
            fonts.font_data.insert(
                "system-body-bold".into(),
                Arc::new(FontData::from_owned(bytes)),
            );
            bold.insert(0, "system-body-bold".into());
            break;
        }
    }
    bold.extend(fallbacks.iter().cloned());
    fonts
        .families
        .insert(FontFamily::Name("body-bold".into()), bold);
    for family in ["heading", "panel-title", "panel-header"] {
        fonts
            .families
            .entry(FontFamily::Name(family.into()))
            .or_default()
            .extend(fallbacks.iter().cloned());
    }
    for (name, paths) in [
        (
            "system-symbols",
            &[
                "/System/Library/Fonts/Apple Symbols.ttf",
                "C:/Windows/Fonts/seguisym.ttf",
                "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
            ][..],
        ),
        (
            "system-cjk",
            &[
                "/System/Library/Fonts/PingFang.ttc",
                "C:/Windows/Fonts/msyh.ttc",
                "C:/Windows/Fonts/simsun.ttc",
                "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
                "/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc",
            ][..],
        ),
    ] {
        if let Some((path, bytes)) = paths
            .iter()
            .find_map(|path| std::fs::read(path).ok().map(|bytes| (*path, bytes)))
        {
            let mut data = FontData::from_owned(bytes);
            if path.ends_with("/noto/NotoSansCJK-Regular.ttc") {
                data.index = 2; // Simplified Chinese face in the Noto CJK collection.
            }
            fonts.font_data.insert(name.into(), Arc::new(data));
            for family in fonts.families.values_mut() {
                family.push(name.into());
            }
        }
    }
    fonts.font_data.insert(
        "bundled-cjk".into(),
        Arc::new(FontData::from_static(BUNDLED_CJK)),
    );
    for family in fonts.families.values_mut() {
        family.push("bundled-cjk".into());
    }
    ctx.set_fonts(fonts);
    let mut style = (*ctx.style()).clone();
    style.visuals = egui::Visuals::dark();
    style.visuals.panel_fill = DARK;
    style.visuals.window_fill = MEDIUM;
    style.visuals.extreme_bg_color = DARK;
    style.visuals.faint_bg_color = MEDIUM;
    style.visuals.override_text_color = Some(TEXT);
    style.visuals.selection.bg_fill = LIGHT;
    style.visuals.selection.stroke.color = ACCENT;
    style.visuals.widgets.inactive.bg_fill = MEDIUM;
    style.visuals.widgets.inactive.weak_bg_fill = MEDIUM;
    style.visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0_f32, LIGHT);
    style.visuals.widgets.hovered.bg_fill = LIGHT;
    style.visuals.widgets.active.bg_fill = LIGHT;
    style.visuals.widgets.active.fg_stroke.color = ACCENT;
    style.spacing.item_spacing = egui::vec2(6.0, 5.0);
    style.spacing.button_padding = egui::vec2(9.0, 6.0);
    style
        .text_styles
        .insert(TextStyle::Body, FontId::proportional(15.0));
    style
        .text_styles
        .insert(TextStyle::Button, FontId::proportional(15.0));
    style
        .text_styles
        .insert(TextStyle::Small, FontId::proportional(12.0));
    style.text_styles.insert(
        TextStyle::Heading,
        FontId::new(22.0, FontFamily::Name("heading".into())),
    );
    ctx.set_style(style);
    egui_extras::install_image_loaders(ctx);
}

pub fn color(value: &str) -> Color32 {
    let hex = value.trim_start_matches('#');
    u32::from_str_radix(hex, 16)
        .map(|n| Color32::from_rgb((n >> 16) as u8, (n >> 8) as u8, n as u8))
        .unwrap_or(LIGHT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "linux")]
    #[test]
    fn linux_body_prefers_installed_sans_without_replacing_condensed_headings() {
        if ![
            "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
            "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf",
        ]
        .iter()
        .all(|path| std::path::Path::new(path).is_file())
        {
            return;
        }

        let ctx = egui::Context::default();
        configure(&ctx);
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            ctx.fonts(|fonts| {
                let families = &fonts.definitions().families;
                assert_eq!(families[&FontFamily::Proportional][0], "system-body");
                assert_eq!(
                    families[&FontFamily::Name("body-bold".into())][0],
                    "system-body-bold"
                );
                assert_eq!(
                    families[&FontFamily::Name("heading".into())][0],
                    "ldtk-bold"
                );
                assert_eq!(
                    families[&FontFamily::Name("panel-title".into())][0],
                    "ldtk-light"
                );
            });
        });
    }

    #[test]
    fn bundled_cjk_covers_the_simplified_chinese_interface_without_system_fonts() {
        let ctx = egui::Context::default();
        let mut fonts = FontDefinitions::empty();
        fonts.font_data.insert(
            "bundled-cjk".into(),
            Arc::new(FontData::from_static(BUNDLED_CJK)),
        );
        fonts
            .families
            .insert(FontFamily::Proportional, vec!["bundled-cjk".into()]);
        ctx.set_fonts(fonts);
        let messages: std::collections::HashMap<String, String> =
            serde_json::from_str(include_str!("../assets/locales/zh-CN.json")).unwrap();
        let glyphs: std::collections::BTreeSet<_> = messages
            .values()
            .flat_map(|message| message.chars())
            .filter(|ch| matches!(*ch as u32, 0x3000..=0x30ff | 0x3400..=0x9fff | 0xff00..=0xffef))
            .collect();
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            ctx.fonts_mut(|fonts| {
                for ch in &glyphs {
                    assert!(
                        fonts.has_glyph(&FontId::proportional(14.), *ch),
                        "missing {ch}"
                    );
                }
            });
        });
    }

    #[test]
    fn system_light_theme_does_not_replace_the_source_dark_editor_style() {
        let ctx = egui::Context::default();
        let _ = ctx.run(
            egui::RawInput {
                system_theme: Some(egui::Theme::Dark),
                ..Default::default()
            },
            |_| {},
        );
        configure(&ctx);
        let _ = ctx.run(
            egui::RawInput {
                system_theme: Some(egui::Theme::Light),
                ..Default::default()
            },
            |ctx| {
                let style = ctx.style();
                assert!(style.visuals.dark_mode);
                assert_eq!(style.visuals.extreme_bg_color, DARK);
                assert_eq!(style.visuals.widgets.inactive.bg_fill, MEDIUM);
            },
        );
    }
}
