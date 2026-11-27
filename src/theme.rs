use eframe::egui::{self, Color32, FontFamily, FontId, TextStyle};

#[path = "theme/fonts.rs"]
mod fonts;
pub(crate) use fonts::set_font_language;
#[cfg(test)]
pub(crate) use fonts::use_reference_layout_fonts;

pub const DARK: Color32 = Color32::from_rgb(0x1e, 0x22, 0x29);
pub const MEDIUM: Color32 = Color32::from_rgb(0x2e, 0x33, 0x3f);
pub const LIGHT: Color32 = Color32::from_rgb(0x54, 0x5d, 0x73);
pub const TEXT: Color32 = Color32::from_rgb(0xc1, 0xcf, 0xeb);
pub const ACCENT: Color32 = Color32::from_rgb(0xff, 0xcc, 0x00);
pub const HELP_TEXT: Color32 = Color32::from_rgb(121, 221, 138);
pub const HELP_FILL: Color32 = Color32::from_rgb(60, 97, 72);
pub const TOOLTIP_TEXT_WIDTH: f32 = 250.;

/// Background and accent for each kind of toast.
pub(crate) mod notice {
    use super::*;

    pub const ERROR: (Color32, Color32) = (Color32::from_rgb(87, 0, 0), Color32::RED);
    pub const SUCCESS: (Color32, Color32) = (
        Color32::from_rgb(22, 62, 38),
        Color32::from_rgb(66, 183, 113),
    );
    pub const WARNING: (Color32, Color32) = (
        Color32::from_rgb(69, 48, 6),
        Color32::from_rgb(203, 141, 19),
    );
    pub const INFO: (Color32, Color32) = (DARK, LIGHT);
    pub const CLIPBOARD: (Color32, Color32) = (Color32::BLACK, Color32::BLACK);
}

/// Text drawn by editor controls that have a more specific role than egui's Body or Heading.
pub(crate) enum TextRole {
    DialogTitle,
    SectionTitle,
    StrongBody,
    OptionTitle,
    PropertyLabel,
    FormValue,
    TooltipBody,
    TooltipHeading,
    NoticeBody,
    NoticeTitle,
    QuickNotice,
}

impl TextRole {
    pub(crate) fn font(self) -> FontId {
        let (size, family) = match self {
            Self::DialogTitle => (20., "panel-title"),
            Self::SectionTitle => (17.333, "panel-title"),
            Self::StrongBody => (15., "body-bold"),
            Self::OptionTitle => (13., "body-bold"),
            Self::PropertyLabel => return FontId::proportional(12.667),
            Self::FormValue => return FontId::proportional(13.333),
            Self::TooltipBody | Self::NoticeBody => return FontId::proportional(14.667),
            Self::TooltipHeading => (17.6, "heading"),
            Self::NoticeTitle => (14.667, "body-bold"),
            Self::QuickNotice => (40., "panel-title"),
        };
        FontId::new(size, FontFamily::Name(family.into()))
    }
}

#[cfg(test)]
pub fn configure(ctx: &egui::Context) {
    configure_for_language(ctx, false);
}

pub(crate) fn configure_for_language(ctx: &egui::Context, chinese: bool) {
    ctx.set_theme(egui::Theme::Dark);
    set_font_language(ctx, chinese);
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
