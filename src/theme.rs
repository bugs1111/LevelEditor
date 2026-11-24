use eframe::egui::{self, Color32, FontFamily, FontId, TextStyle};

#[path = "theme/fonts.rs"]
mod fonts;
pub(crate) use fonts::set_font_language;

pub const DARK: Color32 = Color32::from_rgb(0x1e, 0x22, 0x29);
pub const MEDIUM: Color32 = Color32::from_rgb(0x2e, 0x33, 0x3f);
pub const LIGHT: Color32 = Color32::from_rgb(0x54, 0x5d, 0x73);
pub const TEXT: Color32 = Color32::from_rgb(0xc1, 0xcf, 0xeb);
pub const ACCENT: Color32 = Color32::from_rgb(0xff, 0xcc, 0x00);

pub fn configure(ctx: &egui::Context) {
    ctx.set_theme(egui::Theme::Dark);
    set_font_language(ctx, false);
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
