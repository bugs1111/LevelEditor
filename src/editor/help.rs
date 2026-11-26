use super::*;
use crate::app::chrome::ActionTooltip as _;

struct ChangelogEntry {
    version: &'static str,
    title: &'static str,
    notes: &'static str,
}
fn changelog_entries() -> &'static [ChangelogEntry] {
    static ENTRIES: std::sync::OnceLock<Vec<ChangelogEntry>> = std::sync::OnceLock::new();
    ENTRIES.get_or_init(|| parse_changelog(include_str!("../../assets/CHANGELOG.md")))
}

pub(super) fn latest_changelog_heading() -> (&'static str, &'static str) {
    let latest = &changelog_entries()[0];
    (latest.version, latest.title)
}

fn parse_changelog(source: &'static str) -> Vec<ChangelogEntry> {
    source
        .trim_start_matches("# ")
        .split("\n# ")
        .map(|section| {
            let (heading, notes) = section.split_once('\n').unwrap_or((section, ""));
            let heading = heading.trim_end_matches('\r');
            let (version, title) = heading.split_once(" - ").unwrap_or((heading, ""));
            ChangelogEntry {
                version,
                title,
                notes,
            }
        })
        .collect()
}
fn changelog_markdown(entry: &ChangelogEntry) -> String {
    changelog_markdown_at(entry, &resource_path("assets/changelogImg"))
}
fn changelog_markdown_at(entry: &ChangelogEntry, root: &std::path::Path) -> String {
    let root = root.to_string_lossy().replace('\\', "/");
    let mut parts = entry.notes.split("![](");
    let mut markdown = parts.next().unwrap_or_default().to_owned();
    for part in parts {
        if let Some((image, after)) = part.split_once(')') {
            markdown.push_str(&format!("![](<file://{root}/{image}>)"));
            markdown.push_str(after);
        } else {
            markdown.push_str("![](");
            markdown.push_str(part);
        }
    }
    markdown
}
pub(super) fn changelog_is_open(ctx: &egui::Context) -> bool {
    ctx.data_mut(|d| d.get_temp::<bool>(egui::Id::new("release-notes-open")))
        .unwrap_or(false)
}

fn release_series(version: &str) -> &str {
    version
        .rsplit_once('.')
        .map_or(version, |(series, _)| series)
}

fn is_major_release(version: &str) -> bool {
    version
        .rsplit_once('.')
        .is_some_and(|(_, patch)| patch.split('-').next() == Some("0"))
}

fn changelog_body(
    ui: &mut egui::Ui,
    cache: &mut egui_commonmark::CommonMarkCache,
    entry: &ChangelogEntry,
) {
    egui_commonmark::CommonMarkViewer::new()
        .max_image_width(Some(ui.available_width() as usize))
        .show(ui, cache, &changelog_markdown(entry));
}

const VIDEO_GUIDES: [(&str, &str); 2] = [
    (
        "O1pmIImzEts",
        "General LDtk introduction by Gamefromscratch",
    ),
    ("ah5MLaU5m8s", "Unity integration by Cammin"),
];

fn request_video_thumbnails(ctx: &egui::Context) {
    let key = egui::Id::new("help-video-thumbnails-requested");
    let first_open = ctx.data_mut(|data| {
        let requested = data.get_temp_mut_or_default::<bool>(key);
        !std::mem::replace(requested, true)
    });
    if !first_open {
        return;
    }
    let ctx = ctx.clone();
    std::thread::spawn(move || {
        let config = ureq::Agent::config_builder()
            .https_only(true)
            .timeout_global(Some(std::time::Duration::from_secs(5)))
            .build();
        let agent: ureq::Agent = config.into();
        let mut loaded = false;
        for (id, _) in VIDEO_GUIDES {
            let url = format!("https://img.youtube.com/vi/{id}/0.jpg");
            let Ok(mut response) = agent.get(&url).call() else {
                continue;
            };
            let Ok(bytes) = response
                .body_mut()
                .with_config()
                .limit(1024 * 1024)
                .read_to_vec()
            else {
                continue;
            };
            if matches!(image::guess_format(&bytes), Ok(image::ImageFormat::Jpeg)) {
                loaded = true;
                ctx.include_bytes(format!("bytes://help-videos/{id}.jpg"), bytes);
                ctx.data_mut(|data| data.insert_temp(egui::Id::new(("help-video", id)), true));
                ctx.request_repaint();
            }
        }
        if !loaded {
            ctx.data_mut(|data| {
                data.insert_temp(egui::Id::new("help-video-thumbnails-unavailable"), true);
            });
            ctx.request_repaint();
        }
    });
}

fn video_card(
    ui: &mut egui::Ui,
    id: &'static str,
    title: &'static str,
    width: f32,
) -> egui::Response {
    let title = locale::tr(ui, title);
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, 182.), Sense::click());
    ui.painter().add(
        egui::epaint::Shadow {
            offset: [0, 0],
            blur: 8,
            spread: 0,
            color: Color32::from_black_alpha(128),
        }
        .as_shape(rect, 0),
    );
    ui.painter().rect_filled(rect, 0., Color32::BLACK);
    let inner = rect.shrink(1.);
    if ui.data(|data| data.get_temp::<bool>(egui::Id::new(("help-video", id)))) == Some(true)
        && let Ok(egui::load::TexturePoll::Ready { texture }) =
            egui::Image::from_uri(format!("bytes://help-videos/{id}.jpg"))
                .load_for_size(ui.ctx(), inner.size())
    {
        let crop = ((inner.size() / texture.size) / (inner.size() / texture.size).max_elem())
            .min(Vec2::splat(1.));
        ui.painter().image(
            texture.id,
            inner,
            Rect::from_center_size(egui::pos2(0.5, 0.5), crop),
            Color32::WHITE,
        );
    } else {
        egui::Image::new(egui::include_image!("../../assets/img/stripesLight.png"))
            .texture_options(egui::TextureOptions::NEAREST_REPEAT)
            .uv(Rect::from_min_size(egui::Pos2::ZERO, inner.size() / 8.))
            .paint_at(ui, inner);
    }
    if response.hovered() {
        ui.painter().rect_stroke(
            rect,
            0.,
            Stroke::new(1_f32, theme::ACCENT),
            StrokeKind::Inside,
        );
    }
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Link, true, title));
    response.action_tooltip(title)
}

enum HelpKey<'a> {
    Key(&'a str),
    Mouse(egui::ImageSource<'static>),
    Text(&'a str),
}

impl<'a> HelpKey<'a> {
    fn parse(token: &'a str) -> Self {
        let mouse = match token {
            "mouseleft" => egui::include_image!("../../assets/icons/mouseLeft.png"),
            "mouseright" => egui::include_image!("../../assets/icons/mouseRight.png"),
            "mousemiddle" => egui::include_image!("../../assets/icons/mouseMiddle.png"),
            "mousewheel" => egui::include_image!("../../assets/icons/mouseWheel.png"),
            "/" | "or" | "to" | "(held)" | "(singleclick)" => return Self::Text(token),
            _ => return Self::Key(token),
        };
        Self::Mouse(mouse)
    }

    fn width(&self, ui: &egui::Ui) -> f32 {
        match self {
            Self::Mouse(_) => 32.,
            Self::Text(text) => {
                ui.painter()
                    .layout_no_wrap(
                        locale::tr(ui, text).into(),
                        FontId::proportional(14.),
                        Color32::WHITE,
                    )
                    .size()
                    .x
                    + 4.
            }
            Self::Key(label) => super::chrome::keycap_label_width(ui, label) + 10.,
        }
    }

    fn paint(&self, ui: &egui::Ui, center: egui::Pos2, width: f32) {
        match self {
            Self::Mouse(source) => {
                egui::Image::new(source.clone())
                    .paint_at(ui, Rect::from_center_size(center, Vec2::splat(32.)));
            }
            Self::Text(text) => {
                ui.painter().text(
                    center,
                    Align2::CENTER_CENTER,
                    locale::tr(ui, text),
                    FontId::proportional(14.),
                    Color32::WHITE,
                );
            }
            Self::Key(label) => {
                let tint = match *label {
                    "CTRL" | "CMD" | "SHIFT" | "ALT" => Color32::from_rgb(255, 92, 130),
                    "ESC" | "DELETE" => Color32::from_rgb(195, 145, 255),
                    name if name
                        .strip_prefix('F')
                        .is_some_and(|n| n.parse::<u8>().is_ok()) =>
                    {
                        Color32::from_rgb(195, 145, 255)
                    }
                    _ => Color32::WHITE,
                };
                super::chrome::paint_keycap(
                    ui,
                    Rect::from_center_size(center, egui::vec2(width, 21.)),
                    label,
                    tint,
                    true,
                );
            }
        }
    }
}

fn help_section(ui: &mut egui::Ui, heading: &str, rows: &[(&str, &str)], major: bool) {
    panels::source_controls::form_heading_line(ui, heading);
    ui.add_space(18.);
    let columns = ui.available_width();
    let rows: Vec<_> = rows
        .iter()
        .map(|&(binding, description)| {
            let keys: Vec<_> = binding
                .split_whitespace()
                .map(HelpKey::parse)
                .map(|key| {
                    let width = key.width(ui);
                    (key, width)
                })
                .collect();
            let width = keys.iter().map(|(_, w)| w + 2.).sum::<f32>();
            (keys, width, description)
        })
        .collect();
    let key_width = rows
        .iter()
        .map(|(_, w, _)| w + if major { 8. } else { 16. })
        .fold(columns / 3., f32::max);
    let indent = if major { 4. } else { 10. };
    for (keys, width, description) in rows {
        let galley = ui.painter().layout(
            locale::tr(ui, description).into(),
            FontId::new(
                if major { 16. } else { 14. },
                egui::FontFamily::Name("panel-title".into()),
            ),
            Color32::WHITE,
            (columns - key_width - 2. * indent - 1.).max(30.),
        );
        let has_mouse = keys.iter().any(|(key, _)| matches!(key, HelpKey::Mouse(_)));
        let key_height = if has_mouse { 40_f32 } else { 31. };
        let height = key_height.max(galley.size().y + 8.);
        let (row, _) = ui.allocate_exact_size(egui::vec2(columns, height), Sense::hover());
        let key_rect = Rect::from_min_size(row.min, egui::vec2(key_width, height));
        ui.painter().rect_filled(key_rect, 4., theme::DARK);
        let value = Rect::from_min_max(egui::pos2(key_rect.max.x + 1., row.min.y), row.max);
        if major {
            ui.painter().rect_filled(value, 4., theme::LIGHT);
        }
        ui.painter().galley(
            egui::pos2(
                value.min.x + indent,
                value.center().y - galley.size().y * 0.5,
            ),
            galley,
            Color32::WHITE,
        );
        let mut x = key_rect.max.x - width - if major { 4. } else { 8. };
        for (key, width) in keys {
            key.paint(
                ui,
                egui::pos2(x + width * 0.5, row.min.y + key_height * 0.5),
                width,
            );
            x += width + 2.;
        }
        ui.add_space(1.);
    }
}

impl EditorApp {
    fn help_link(
        &mut self,
        ui: &egui::Ui,
        response: egui::Response,
        url: &str,
        show_address: bool,
    ) {
        let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
        if show_address
            && response.hovered()
            && !response.secondary_clicked()
            && !egui::Popup::is_any_open(ui.ctx())
        {
            let address = url.strip_prefix("https://").unwrap_or(url);
            chrome::source_tooltip_popup(
                &response,
                Some(egui::emath::RectAlign::RIGHT_START),
                egui::Margin::symmetric(18, 12),
            )
            .show(|ui| {
                ui.label(
                    RichText::new(address)
                        .monospace()
                        .size(9.33)
                        .color(panels::source_controls::HELP_GREEN),
                );
            });
        }
        if response.clicked() || response.middle_clicked() {
            ui.ctx().open_url(egui::OpenUrl::new_tab(url));
            if response.clicked() {
                self.report_canvas_notice(notifications::CanvasNotice::LinkOpened);
            }
        }
        egui::Popup::context_menu(&response)
            .anchor(response.rect)
            .frame(panels::source_controls::context_menu_frame(ui.style()))
            .show(|ui| {
                let label = locale::tr(ui, "Copy URL").to_uppercase();
                let font = FontId::proportional(13.33);
                let galley = ui
                    .painter()
                    .layout_no_wrap(label, font, Color32::PLACEHOLDER);
                let size = egui::vec2(galley.size().x + 40., 31.);
                let (rect, item) = ui.allocate_exact_size(size, Sense::click());
                let color = if item.hovered() {
                    ui.painter().rect_filled(rect, 0., theme::ACCENT);
                    Color32::BLACK
                } else {
                    theme::ACCENT
                };
                item.widget_info(|| {
                    egui::WidgetInfo::labeled(egui::WidgetType::Button, true, galley.text())
                });
                ui.painter()
                    .galley(rect.center() - galley.size() * 0.5, galley, color);
                if item.clicked() {
                    ui.ctx().copy_text(url.to_owned());
                    self.report_canvas_notice(notifications::CanvasNotice::LinkCopied);
                    ui.close();
                }
            });
    }

    pub(super) fn help_contents(&mut self, ui: &mut egui::Ui) {
        request_video_thumbnails(ui.ctx());
        let width = ui.available_width();
        let unavailable = ui
            .data(|data| data.get_temp::<bool>(egui::Id::new("help-video-thumbnails-unavailable")))
            == Some(true);
        let docs_width = if unavailable { 179. } else { 265.5 };
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 16.;
            ui.with_layout(egui::Layout::top_down_justified(egui::Align::Min), |ui| {
                ui.set_width(docs_width);
                ui.add_space(2.);
                panels::source_controls::form_heading_line(ui, "Online documentation");
                ui.add_space(10.);
                for (label, icon, fill, url) in [
                    (
                        "Release notes",
                        egui::include_image!("../../assets/icons/doc.svg"),
                        Color32::from_rgb(227, 77, 0),
                        None,
                    ),
                    (
                        "Website",
                        egui::include_image!("../../assets/icons/doc.svg"),
                        Color32::from_rgb(73, 141, 85),
                        Some("https://ldtk.io/docs"),
                    ),
                    (
                        "JSON file format",
                        egui::include_image!("../../assets/icons/json.svg"),
                        Color32::from_rgb(73, 141, 85),
                        Some("https://ldtk.io/json"),
                    ),
                    (
                        "Discord",
                        egui::include_image!("../../assets/icons/discord.svg"),
                        Color32::from_rgb(114, 137, 218),
                        Some("https://ldtk.io/go/discord"),
                    ),
                ] {
                    let label = locale::tr(ui, label);
                    let button = egui::Button::new("").fill(fill).corner_radius(2.);
                    let response =
                        panels::source_controls::add_sized_button(ui, [docs_width, 43.], button);
                    let margin = response.rect.left_center();
                    egui::Image::new(icon)
                        .tint(Color32::from_white_alpha(if url.is_none() {
                            255
                        } else {
                            190
                        }))
                        .paint_at(
                            ui,
                            Rect::from_center_size(margin + egui::vec2(22., 0.), Vec2::splat(21.)),
                        );
                    ui.painter().text(
                        margin + egui::vec2(37., 0.),
                        Align2::LEFT_CENTER,
                        label,
                        FontId::proportional(13.33),
                        Color32::WHITE,
                    );
                    response.widget_info(|| {
                        egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label)
                    });
                    if let Some(url) = url {
                        self.help_link(ui, response, url, true);
                    } else if response.clicked() {
                        self.dismiss_panel();
                        ui.data_mut(|data| {
                            data.insert_temp(egui::Id::new("release-notes-open"), true)
                        });
                    }
                    ui.add_space(1.);
                }
            });
            ui.vertical(|ui| {
                let video_width = (width - 16. - docs_width).max(80.);
                ui.set_width(video_width);
                panels::source_controls::form_heading_line(ui, "Video guides");
                ui.add_space(11.);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.;
                    for (id, title) in VIDEO_GUIDES {
                        let response = video_card(ui, id, title, (video_width - 4.) * 0.5);
                        self.help_link(ui, response, &format!("https://youtu.be/{id}"), false);
                    }
                });
            });
        });
        ui.add_space(16.);
        let navigation = self.settings.value["navigationKeys"]
            .as_str()
            .unwrap_or("Wasd");
        let ctrl = super::chrome::shortcut_label("CTRL", cfg!(target_os = "macos"));
        let toggle_details = if cfg!(target_os = "macos") {
            "SHIFT H"
        } else {
            "CTRL H"
        };
        let fullscreen = if cfg!(target_os = "macos") {
            "CMD CTRL F"
        } else if cfg!(target_os = "windows") {
            "F11"
        } else {
            "CTRL SHIFT F"
        };
        let single_layer = if super::shortcuts::single_layer_shortcut_needs_shift(navigation) {
            "SHIFT A"
        } else {
            "A"
        };
        let save_tiles = if super::shortcuts::save_tile_shortcut_needs_shift(navigation) {
            "SHIFT S"
        } else {
            "S"
        };
        let world_key = if navigation == "Zqsd" {
            "SHIFT W or ²"
        } else {
            "SHIFT W or ~"
        };

        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 16.;
            let column = 420.;
            ui.vertical(|ui| {
                ui.set_width(column);
                help_section(
                    ui,
                    "Most important commands",
                    &[
                        ("mouseleft", "Draw"),
                        ("mouseright", "Erase"),
                        ("mousemiddle or SPACE mouseleft", "Move the view"),
                        ("mousewheel or SPACE mouseright", "Zoom"),
                        ("+ / -", "Zoom"),
                        ("ESC", "Clear selection, close panel, cancel an action etc."),
                    ],
                    true,
                );
                ui.add_space(14.);
                help_section(
                    ui,
                    "General commands",
                    &[
                        ("H", "Show this help"),
                        ("TAB", "Toggle zen mode"),
                        (&format!("{ctrl} F"), "Search"),
                        (toggle_details, "Show/hide visual details"),
                        ("F1 to F10", "Select layers"),
                        (fullscreen, "Toggle fullscreen"),
                        ("F12", "Open app settings"),
                        (&format!("{ctrl} S"), "Save project"),
                        (&format!("{ctrl} SHIFT S"), "Save project as..."),
                        (&format!("{ctrl} W"), "Close project"),
                        (&format!("{ctrl} Q"), "Quit LDtk"),
                        ("G", "Enable/disable the grid"),
                        (single_layer, "Toggle \"Single layer\" mode"),
                        (&format!("{ctrl} A"), "Select all"),
                        ("DELETE", "Delete selected elements"),
                        (&format!("{ctrl} R"), "Run a project custom command"),
                    ],
                    false,
                );
                ui.add_space(14.);
                help_section(
                    ui,
                    "Sections",
                    &[
                        ("P", "Open \"Project\" panel"),
                        ("L", "Open \"Layers\" panel"),
                        ("E", "Open \"Entities\" panel"),
                        ("U", "Open \"Enumerations\" panel"),
                        ("T", "Open \"Tileset\" panel"),
                    ],
                    false,
                );
            });
            ui.vertical(|ui| {
                ui.set_width(column);
                help_section(
                    ui,
                    "Editing commands",
                    &[
                        ("ALT mouseleft", "Select a single element"),
                        (
                            "SHIFT ALT mouseleft",
                            "Select all elements in a rectangle area",
                        ),
                        (
                            &format!("{ctrl} ALT mouseleft"),
                            "Duplicate all selected elements",
                        ),
                        ("SPACE", "Fit view"),
                        ("SHIFT mouseleft (held)", "Draw a rectangle."),
                        (
                            "SHIFT mouseleft (singleclick)",
                            "Flood-fill the current area with the selected value.",
                        ),
                    ],
                    true,
                );
                ui.add_space(14.);
                help_section(
                    ui,
                    "Auto-layers",
                    &[("SHIFT R", "Toggle auto-layers rendering")],
                    false,
                );
                ui.add_space(14.);
                help_section(
                    ui,
                    "Tile layers",
                    &[
                        (save_tiles, "Save current tile selection."),
                        (
                            "SHIFT L",
                            "Reload a saved tile selection from currently selected tile.",
                        ),
                        ("R", "Toggle randomization of current tile selection."),
                        ("X", "X-mirror current tile(s)"),
                        ("Y", "Y-mirror current tile(s)"),
                    ],
                    false,
                );
                ui.add_space(14.);
                help_section(
                    ui,
                    "World mode",
                    &[
                        (world_key, "Switch between world/level views"),
                        ("PGUP / PGDN", "Change active world layer"),
                        (
                            &format!("{ctrl} PGUP / {ctrl} PGDN"),
                            "Move current level between world layers",
                        ),
                    ],
                    false,
                );
            });
        });
    }
    pub(super) fn changelog_dialog(&mut self, ctx: &egui::Context) {
        let id = egui::Id::new("release-notes-open");
        if !changelog_is_open(ctx) {
            return;
        }
        let entries = changelog_entries();
        let version_id = id.with("version");
        let default = entries
            .iter()
            .find(|entry| is_major_release(entry.version))
            .unwrap_or(&entries[0]);
        let mut version = ctx
            .data(|data| data.get_temp::<String>(version_id))
            .unwrap_or_else(|| default.version.into());
        let entry = entries
            .iter()
            .find(|entry| entry.version == version)
            .unwrap_or(default);
        let series = release_series(entry.version);
        let patches: Vec<_> = entries
            .iter()
            .filter(|patch| {
                release_series(patch.version) == series && !is_major_release(patch.version)
            })
            .collect();
        let newest = patches.first().map_or(entry.version, |patch| patch.version);
        let width = ctx.content_rect().width().min(900.);
        let height = ctx.content_rect().height() * 0.96;
        let cache = ctx.data_mut(|data| {
            data.get_temp_mut_or_default::<std::sync::Arc<std::sync::Mutex<egui_commonmark::CommonMarkCache>>>(id.with("cache")).clone()
        });

        let mut open = true;
        let modal = egui::Modal::new(id)
            .frame(egui::Frame::new().fill(theme::DARK).corner_radius(4.))
            .backdrop_color(Color32::from_black_alpha(220))
            .show(ctx, |ui| {
                ui.set_width(width);
                ui.set_height(height);
                ui.spacing_mut().item_spacing = Vec2::ZERO;
                let (header, _) = ui.allocate_exact_size(egui::vec2(width, 105.), Sense::hover());
                let badge = Rect::from_min_size(header.min, egui::vec2(161., 105.));
                ui.painter().rect_filled(header, 0., theme::MEDIUM);
                ui.painter().rect_filled(badge, 0., theme::ACCENT);
                ui.painter().text(
                    badge.center(),
                    Align2::CENTER_CENTER,
                    newest,
                    FontId::new(68., egui::FontFamily::Name("panel-title".into())),
                    theme::DARK,
                );
                if !entry.title.is_empty() {
                    ui.painter().text(
                        header.left_center() + egui::vec2(181., 0.),
                        Align2::LEFT_CENTER,
                        format!("“ {} ”", entry.title),
                        FontId::new(26., egui::FontFamily::Name("panel-title".into())),
                        theme::ACCENT,
                    );
                }
                let button_pos = header.right_top() + egui::vec2(-52., 9.);
                let button_size = egui::vec2(45., 42.);
                let orange = Color32::from_rgb(235, 78, 0);
                if ui
                    .put(
                        Rect::from_min_size(button_pos, button_size),
                        egui::Button::new(RichText::new("×").size(24.).color(Color32::WHITE))
                            .fill(orange),
                    )
                    .clicked()
                {
                    open = false;
                }
                let others = ui
                    .put(
                        Rect::from_min_size(button_pos + egui::vec2(0., 44.), button_size),
                        egui::Button::image(
                            egui::Image::new(egui::include_image!("../../assets/icons/list.svg"))
                                .fit_to_exact_size(Vec2::splat(20.)),
                        )
                        .fill(orange),
                    )
                    .action_tooltip(locale::tr(ui, "All updates"));
                egui::Popup::menu(&others)
                    .align(egui::RectAlign::BOTTOM_END)
                    .frame(panels::source_controls::context_menu_frame(ui.style()).inner_margin(6))
                    .show(|ui| {
                        ui.label(RichText::new(locale::tr(ui, "LDtk major updates")).strong());
                        for release in entries
                            .iter()
                            .filter(|entry| is_major_release(entry.version))
                        {
                            let series = release_series(release.version);
                            let title = if release.title.is_empty() {
                                series.into()
                            } else {
                                format!("{series} - {}", release.title)
                            };
                            if ui
                                .selectable_label(version == release.version, title)
                                .clicked()
                            {
                                version = release.version.into();
                                ui.close();
                            }
                        }
                    });

                ui.spacing_mut().scroll = egui::style::ScrollStyle {
                    bar_width: 10.,
                    bar_inner_margin: 0.,
                    bar_outer_margin: 0.,
                    ..egui::style::ScrollStyle::solid()
                };
                egui::ScrollArea::vertical()
                    .id_salt(("changelog-body", entry.version))
                    .max_height((height - header.height()).max(120.))
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        egui::Frame::new().inner_margin(20).show(ui, |ui| {
                            ui.spacing_mut().item_spacing.y = 8.;
                            let mut cache = cache.lock().unwrap();
                            for patch in patches {
                                let latest = entries[0].version == patch.version;
                                egui::Frame::new()
                                    .fill(if latest { theme::LIGHT } else { theme::MEDIUM })
                                    .stroke(Stroke::new(
                                        1_f32,
                                        if latest { theme::TEXT } else { theme::LIGHT },
                                    ))
                                    .corner_radius(2.)
                                    .inner_margin(8)
                                    .show(ui, |ui| {
                                        ui.set_min_width(ui.available_width());
                                        egui::CollapsingHeader::new(
                                            RichText::new(locale::format(
                                                ui,
                                                "Patch {version}",
                                                &[("version", patch.version)],
                                            ))
                                            .strong()
                                            .color(theme::ACCENT),
                                        )
                                        .id_salt(patch.version)
                                        .default_open(latest)
                                        .show(ui, |ui| {
                                            changelog_body(ui, &mut cache, patch);
                                        });
                                    });
                            }
                            if newest != entry.version {
                                ui.heading(locale::format(
                                    ui,
                                    "Changes from {version}",
                                    &[("version", entry.version)],
                                ));
                            }
                            changelog_body(ui, &mut cache, entry);
                        });
                    });
            });
        open &= !modal.should_close();
        ctx.data_mut(|data| {
            data.insert_temp(id, open);
            if open {
                data.insert_temp(version_id, version);
            } else {
                data.remove::<String>(version_id);
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn help_keeps_both_shortcut_columns_at_the_original_positions_in_narrow_windows() {
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        ctx.data_mut(|data| {
            data.insert_temp(egui::Id::new("help-video-thumbnails-requested"), true)
        });
        let mut app = EditorApp::empty();
        app.set_project(ldtk_core::Project::new());
        app.panel = Some(Panel::Help);
        let mut draw = |width, events| {
            ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(width, 600.),
                    )),
                    events,
                    ..Default::default()
                },
                |ctx| assert!(app.source_panel(ctx, Panel::Help)),
            )
        };
        let heading_x = |output: &egui::FullOutput, heading: &str| {
            output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) if text.galley.text() == heading => Some(text.pos.x),
                    _ => None,
                })
                .expect(heading)
        };
        for width in [1100., 800.] {
            draw(width, vec![]);
            draw(width, vec![]);
            let output = draw(width, vec![]);
            for (heading, expected) in [
                ("VIDEO GUIDES", 301.5),
                ("MOST IMPORTANT COMMANDS", 20.),
                ("EDITING COMMANDS", 456.),
            ] {
                let x = heading_x(&output, heading);
                assert!(
                    (x - expected).abs() < 1.,
                    "{heading} starts at {x} with window width {width}"
                );
            }
            for key in ["ESC", "SPACE", "SHIFT", "ALT"] {
                assert!(output.shapes.iter().any(|shape| {
                    matches!(&shape.shape, egui::Shape::Text(text) if text.galley.text() == key)
                }));
            }
        }
        ctx.data_mut(|data| {
            data.insert_temp(egui::Id::new("help-video-thumbnails-unavailable"), true);
        });
        let unavailable = draw(800., vec![]);
        assert!((heading_x(&unavailable, "VIDEO GUIDES") - 215.).abs() < 1.);
        assert!((heading_x(&unavailable, "EDITING COMMANDS") - 456.).abs() < 1.);
        draw(
            800.,
            vec![
                egui::Event::PointerMoved(egui::pos2(500., 320.)),
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(-240., 0.),
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        let scrolled = draw(800., vec![]);
        assert!(heading_x(&scrolled, "EDITING COMMANDS") < 440.);
    }

    #[test]
    fn help_website_link_supports_middle_click_and_copy_url() {
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        ctx.data_mut(|data| {
            data.insert_temp(egui::Id::new("help-video-thumbnails-requested"), true)
        });
        let mut app = EditorApp::empty();
        app.panel = Some(Panel::Help);
        let mut draw = |events| {
            ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(856., 850.),
                    )),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    app.shortcuts(ctx);
                    if app.panel == Some(Panel::Help) {
                        egui::CentralPanel::default().show(ctx, |ui| app.help_contents(ui));
                    }
                },
            )
        };
        let text_position = |output: &egui::FullOutput, label: &str| {
            output.shapes.iter().find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text() == label => {
                    Some(text.pos + text.galley.size() * 0.5)
                }
                _ => None,
            })
        };
        let pointer = |pos, button, pressed| {
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ]
        };
        let initial = draw(vec![]);
        assert!(text_position(&initial, "ldtk.io/docs").is_none());
        let website = text_position(&initial, "Website").unwrap();
        draw(pointer(website, egui::PointerButton::Middle, true));
        let opened = draw(pointer(website, egui::PointerButton::Middle, false));
        let url = "https://ldtk.io/docs";
        assert!(opened.platform_output.commands.iter().any(|command| {
            matches!(command, egui::OutputCommand::OpenUrl(open) if open.url == url)
        }));

        draw(pointer(website, egui::PointerButton::Secondary, true));
        draw(pointer(website, egui::PointerButton::Secondary, false));
        let menu = draw(vec![]);
        assert!(text_position(&menu, "ldtk.io/docs").is_none());
        let copy = text_position(&menu, "COPY URL").expect("help link context action");
        draw(pointer(copy, egui::PointerButton::Primary, true));
        let copied = draw(pointer(copy, egui::PointerButton::Primary, false));
        assert!(copied.platform_output.commands.iter().any(|command| {
            matches!(command, egui::OutputCommand::CopyText(text) if text == url)
        }));

        draw(pointer(website, egui::PointerButton::Secondary, true));
        draw(pointer(website, egui::PointerButton::Secondary, false));
        assert!(egui::Popup::is_any_open(&ctx));
        let escape = draw(vec![egui::Event::Key {
            key: Key::Escape,
            physical_key: Some(Key::Escape),
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }]);
        assert!(!egui::Popup::is_any_open(&ctx));
        assert!(text_position(&escape, "Website").is_some());
    }

    #[test]
    fn opening_release_notes_from_help_closes_the_help_panel() {
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        ctx.data_mut(|data| {
            data.insert_temp(egui::Id::new("help-video-thumbnails-requested"), true)
        });
        let mut app = EditorApp::empty();
        app.panel = Some(Panel::Help);
        let mut draw = |events| {
            ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(866., 775.),
                    )),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| app.help_contents(ui));
                },
            )
        };
        let output = draw(vec![]);
        let pos = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text() == "Release notes" => {
                    Some(text.pos + text.galley.size() * 0.5)
                }
                _ => None,
            })
            .expect("visible release notes button");
        for pressed in [true, false] {
            let _ = draw(vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ]);
        }
        assert!(changelog_is_open(&ctx));
        assert!(app.panel.is_none());
    }

    #[test]
    fn release_notes_initial_view_opens_the_154_patch() {
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        ctx.data_mut(|data| data.insert_temp(egui::Id::new("release-notes-open"), true));
        let mut app = EditorApp::empty();
        let mut found = false;
        for _ in 0..3 {
            let output = ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1440., 900.),
                    )),
                    ..Default::default()
                },
                |ctx| app.changelog_dialog(ctx),
            );
            found |= output.shapes.iter().any(|shape| {
                matches!(&shape.shape, egui::Shape::Text(text) if text.galley.text() == "Patch 1.5.4")
            });
        }
        assert!(found, "the preview patch is visible in the initial window");
    }

    #[derive(Default)]
    struct ImageRequests(std::sync::Mutex<Vec<String>>);
    impl egui::load::BytesLoader for ImageRequests {
        fn id(&self) -> &str {
            egui::generate_loader_id!(ImageRequests)
        }
        fn load(&self, _: &egui::Context, uri: &str) -> egui::load::BytesLoadResult {
            self.0.lock().unwrap().push(uri.to_owned());
            Err(egui::load::LoadError::NotSupported)
        }
        fn forget(&self, _: &str) {}
        fn forget_all(&self) {}
        fn byte_size(&self) -> usize {
            0
        }
    }
    #[test]
    fn release_images_load_through_commonmark_from_a_bundle_path_with_spaces_and_hashes() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir
            .path()
            .join("Level Editor #test.app/Contents/Resources/assets/changelogImg");
        std::fs::create_dir_all(&root).unwrap();
        let png = include_bytes!("../../assets/icons/intGrid.png");
        let image = root.join("logo#1.png");
        std::fs::write(&image, png).unwrap();
        let entry = ChangelogEntry {
            version: "1.5.0",
            title: "Fixture",
            notes: "Before.\n\n![](logo#1.png)\n\n[Unchanged link](https://example.test/help)\n",
        };
        let markdown = changelog_markdown_at(&entry, &root);
        let expected = format!("file://{}", image.to_string_lossy().replace('\\', "/"));
        let ctx = egui::Context::default();
        egui_extras::install_image_loaders(&ctx);
        let requests = std::sync::Arc::new(ImageRequests::default());
        ctx.add_bytes_loader(requests.clone());
        let mut cache = egui_commonmark::CommonMarkCache::default();
        let decoded = image::load_from_memory(png).unwrap();
        let size = [decoded.width() as usize, decoded.height() as usize];
        let mut loaded = false;
        let mut draw = || {
            let output = ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(800., 600.),
                    )),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        egui_commonmark::CommonMarkViewer::new().show(ui, &mut cache, &markdown);
                    });
                },
            );
            output
                .textures_delta
                .set
                .iter()
                .any(|(id, delta)| *id != egui::TextureId::Managed(0) && delta.image.size() == size)
        };
        for _ in 0..3 {
            loaded |= draw();
        }
        let observed = requests.0.lock().unwrap().clone();
        assert!(
            observed.contains(&expected),
            "CommonMark must request the complete raw file URI: {observed:?}"
        );
        for _ in 0..100 {
            if loaded {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
            loaded |= draw();
        }
        assert!(
            loaded,
            "the real file/image loaders must decode the bundled PNG without URI percent-encoding"
        );
        assert!(markdown.contains("[Unchanged link](https://example.test/help)"));
    }
    #[test]
    fn windows_release_notes_keep_clean_versions_and_titles() {
        let entries = parse_changelog(
            "# 1.5.3\r\n\r\n- Fixed\r\n\r\n# 1.5.0 - New LDtk logo\r\n\r\n![](logo.png)\r\n",
        );
        assert_eq!(entries[0].version, "1.5.3");
        assert_eq!(entries[1].version, "1.5.0");
        assert_eq!(entries[1].title, "New LDtk logo");
        assert!(entries[1].notes.contains("![](logo.png)"));
    }
    #[test]
    fn original_release_entries_preserve_versions_text_and_bundled_images() {
        let entries = changelog_entries();
        assert_eq!(entries[0].version, "1.5.4");
        assert_eq!(entries[1].version, "1.5.3");
        let major = entries.iter().find(|e| e.version == "1.5.0").unwrap();
        assert_eq!(major.title, "New LDtk logo");
        let markdown = changelog_markdown(major);
        assert!(markdown.contains("file://"));
        for image in major
            .notes
            .split("![](")
            .skip(1)
            .filter_map(|s| s.split_once(')').map(|s| s.0))
        {
            assert!(
                resource_path(&format!("assets/changelogImg/{image}")).is_file(),
                "{image}"
            );
        }
    }
}
