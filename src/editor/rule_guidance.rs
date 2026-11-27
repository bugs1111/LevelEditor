use super::*;
use source_controls::{help_card as card, help_paragraph as paragraph};

pub(super) const MUTED: Color32 = Color32::from_rgb(115, 127, 157);
pub(super) const EDITOR_WIDTH: f32 = 662.;
const SIDE_WIDTH: f32 = 400.;
pub(super) const WIDTH: f32 = EDITOR_WIDTH + 8. + SIDE_WIDTH;

pub(super) struct GuideRows {
    pub output: f32,
    pub pattern_and_explanation: f32,
}

pub(super) fn describe(ctx: &egui::Context, source: &Value, value: i64) -> Option<String> {
    if value == 0 {
        return Some(locale::tr_ctx(ctx, "This cell content doesn't matter.").into());
    }
    let id = value.saturating_abs();
    let item = if id == ANY_VALUE {
        locale::tr_ctx(ctx, "any IntGrid value").into()
    } else if id >= 1_000 {
        let uid = id / 1_000 - 1;
        let title = rule_value_group(source, uid).map_or_else(
            || locale::tr_ctx(ctx, "Ungrouped").to_owned(),
            |group| {
                group["identifier"].as_str().map_or_else(
                    || locale::format_ctx(ctx, "Group {uid}", &[("uid", &uid.to_string())]),
                    str::to_owned,
                )
            },
        );
        locale::format_ctx(
            ctx,
            "any IntGrid value from the group {name}",
            &[("name", &title)],
        )
    } else {
        let entry = source["intGridValues"]
            .as_array()?
            .iter()
            .find(|entry| entry["value"] == id)?;
        let title = entry["identifier"]
            .as_str()
            .map_or_else(|| id.to_string(), |name| format!("{name} ({id})"));
        format!("\"{title}\"")
    };
    let message = if value < 0 {
        "This cell should NOT contain {value} to match."
    } else {
        "This cell should contain {value} to match."
    };
    Some(locale::format_ctx(ctx, message, &[("value", &item)]))
}

pub(super) fn explanation(ui: &mut egui::Ui, width: f32, selected: Option<String>) -> Rect {
    let active = selected.is_some();
    let message = selected.as_deref().unwrap_or_else(|| {
        locale::tr(ui, "Move your cursor over a cell in the pattern to get more explanations about its meaning.")
    });
    egui::Frame::new()
        .fill(if active {
            theme::HELP_FILL
        } else {
            theme::DARK
        })
        .corner_radius(3)
        .inner_margin(16)
        .show(ui, |ui| {
            let size = egui::vec2(width.min(580.) - 32., 54.);
            ui.set_min_size(size);
            ui.allocate_ui_with_layout(
                size,
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    let label = RichText::new(message).size(13.333).color(if active {
                        theme::HELP_TEXT
                    } else {
                        MUTED
                    });
                    ui.add(egui::Label::new(if active { label } else { label.italics() }).wrap());
                },
            );
        })
        .response
        .rect
}

pub(super) fn show(ui: &mut egui::Ui, editor: impl FnOnce(&mut egui::Ui) -> GuideRows) -> Rect {
    ui.add_space(8.);
    let intro = card(ui, WIDTH, 95., |ui| {
        paragraph(
            ui,
            "A rule is used to display a specific tile based on the content of an IntGrid layer.",
        );
        paragraph(
            ui,
            "For example, you could create a rule that will display \"grass\" tiles above every walls.",
        );
        paragraph(
            ui,
            "To describe the \"context\" (ie. where and when your tile should be displayed), you paint a pattern: that's the grid below.",
        );
    });
    let body = ui.horizontal_top(|ui| {
        let rows = ui
            .vertical(|ui| {
                ui.set_width(EDITOR_WIDTH);
                editor(ui)
            })
            .inner;
        ui.vertical(|ui| {
            ui.set_width(SIDE_WIDTH);
            ui.add_space(8.);
            output_help(ui, SIDE_WIDTH, rows.output - 8.);
            ui.add_space(8.);
            pattern_help(ui, SIDE_WIDTH, rows.pattern_and_explanation - 8.);
        });
    });
    let content = intro.union(body.response.rect);
    Rect::from_min_max(
        content.min - egui::vec2(8., 16.),
        content.max + Vec2::splat(8.),
    )
}

fn guide_text(message: &str, size: f32) -> RichText {
    RichText::new(message)
        .font(FontId::new(
            size,
            egui::FontFamily::Name("panel-title".into()),
        ))
        .color(theme::HELP_TEXT)
}

fn output_help(ui: &mut egui::Ui, width: f32, height: f32) {
    card(ui, width, height, |ui| {
        paragraph(
            ui,
            "First pick which tile(s) you would like to automatically render. Start with something very simple: a wall block, grass, etc.",
        );
    });
}

fn pattern_help(ui: &mut egui::Ui, width: f32, height: f32) {
    card(ui, width, height, |ui| {
        paragraph(
            ui,
            "The center of this PATTERN is where your TILE(s) will be rendered (). Paint the pattern to describe in which context the tile(s) should be displayed:",
        );
        ui.indent("rule-help-mouse", |ui| {
            ui.spacing_mut().item_spacing.y = 2.;
            for (icon, description) in [
                (egui::include_image!("../../assets/icons/mouseLeft.png"), "paint \"This IntGrid value is required\" in a cell"),
                (egui::include_image!("../../assets/icons/mouseRight.png"), "paint \"This IntGrid value should NOT be here\"."),
                (egui::include_image!("../../assets/icons/mouseMiddle.png"), "clear pattern cell."),
            ] {
                mouse_row(ui, Some(icon), description);
            }
            mouse_row(ui, None, "Use the Any Value special value to match \"Any IntGrid value\" or \"No IntGrid value at all\".");
        });
        paragraph(
            ui,
            "For example, if you want to paint \"wall blocks\" where you have the IntGrid value \"1\" in your level, you just put the IntGrid value \"1\" in the center of this pattern.",
        );
        paragraph(
            ui,
            "This means: \"if any cell in my level contain value 1, paint the given tile(s) there\".",
        );
    });
}

fn mouse_row(ui: &mut egui::Ui, icon: Option<egui::ImageSource<'static>>, description: &str) {
    egui::Frame::new()
        .fill(theme::DARK)
        .corner_radius(2)
        .inner_margin(6)
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.;
                if let Some(icon) = icon {
                    ui.add(egui::Image::new(icon).fit_to_exact_size(Vec2::splat(16.)));
                }
                ui.add(egui::Label::new(guide_text(locale::tr(ui, description), 12.)).wrap());
            });
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explanations_follow_the_hovered_condition_and_source_names() {
        let ctx = egui::Context::default();
        let source = json!({
            "intGridValues": [{"value": 1, "identifier": "wall"}, {"value": 3, "identifier": null}],
            "intGridValuesGroups": [{"uid": 1, "identifier": "Terrain"}, {"uid": 2, "identifier": null}]
        });
        for (value, expected) in [
            (0, "This cell content doesn't matter."),
            (1, "This cell should contain \"wall (1)\" to match."),
            (-3, "This cell should NOT contain \"3\" to match."),
            (
                ANY_VALUE,
                "This cell should contain any IntGrid value to match.",
            ),
            (
                -ANY_VALUE,
                "This cell should NOT contain any IntGrid value to match.",
            ),
            (
                1_000,
                "This cell should contain any IntGrid value from the group Ungrouped to match.",
            ),
            (
                -2_000,
                "This cell should NOT contain any IntGrid value from the group Terrain to match.",
            ),
            (
                3_000,
                "This cell should contain any IntGrid value from the group Group 2 to match.",
            ),
        ] {
            assert_eq!(describe(&ctx, &source, value).as_deref(), Some(expected));
        }
        assert_eq!(describe(&ctx, &source, 9), None);
        locale::set(&ctx, locale::Language::SimplifiedChinese);
        assert_eq!(
            describe(&ctx, &source, -2_000).as_deref(),
            Some("此格不能包含「Terrain」组中的任何整数网格值。")
        );
        assert_eq!(
            describe(&ctx, &source, 1).as_deref(),
            Some("此格必须包含 \"wall (1)\" 才能匹配。")
        );
    }
}
