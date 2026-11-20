use super::*;
use std::collections::BTreeMap;
const STANDARD: [u32; 32] = [
    0xbe4a2f, 0xd77643, 0xead4aa, 0xe4a672, 0xb86f50, 0x733e39, 0x3e2731, 0xa22633, 0xe43b44,
    0xf77622, 0xfeae34, 0xfee761, 0x63c74d, 0x3e8948, 0x265c42, 0x193c3e, 0x124e89, 0x0099db,
    0x2ce8f5, 0xffffff, 0xc0cbdc, 0x8b9bb4, 0x5a6988, 0x3a4466, 0x262b44, 0x181425, 0xff0044,
    0x68386c, 0xb55088, 0xf6757a, 0xe8b796, 0xc28569,
];
const COLORBLIND: [u32; 16] = [
    0x000000, 0x252525, 0x676767, 0xffffff, 0x171723, 0x004949, 0x009999, 0x22cf22, 0x490092,
    0x006ddb, 0xb66dff, 0xff6db6, 0x920000, 0x8f4e00, 0xdb6d00, 0xffdf4d,
];
pub(in crate::app) fn palette(color_blind: bool) -> &'static [u32] {
    if color_blind { &COLORBLIND } else { &STANDARD }
}
fn rgb(color: u32) -> [u8; 3] {
    [(color >> 16) as u8, (color >> 8) as u8, color as u8]
}
fn distance(a: u32, b: u32) -> f64 {
    let a = rgb(a);
    let b = rgb(b);
    (0..3)
        .map(|i| (f64::from(a[i]) - f64::from(b[i])).abs() / 255.)
        .sum::<f64>()
        / 3.
}
fn suggest(color_blind: bool, used: &[u32]) -> u32 {
    let choices = palette(color_blind);
    let mut counts = BTreeMap::<u32, usize>::new();
    for color in used {
        *counts.entry(*color).or_default() += 1;
    }
    for color in used {
        for nice in choices {
            if distance(*color, *nice) < 0.1 {
                *counts.entry(*nice).or_default() += 1;
            }
        }
    }
    choices
        .iter()
        .find(|color| !counts.contains_key(color))
        .copied()
        .unwrap_or_else(|| {
            *choices
                .iter()
                .min_by_key(|color| counts.get(color).copied().unwrap_or(0))
                .expect("nonempty original palette")
        })
}
impl EditorApp {
    pub(super) fn suggested_color(&self, used: &[Value]) -> u32 {
        let used = used
            .iter()
            .map(|value| {
                let [r, g, b] = enum_color(value);
                u32::from(r) << 16 | u32::from(g) << 8 | u32::from(b)
            })
            .collect::<Vec<_>>();
        suggest(self.settings.bool("colorBlind"), &used)
    }
}
#[derive(Clone, Default)]
struct UsedColors(BTreeMap<String, Vec<u32>>);
fn color_value(value: &Value) -> Option<u32> {
    let rgb = source_hex(value.as_str()?)?;
    Some(u32::from(rgb[0]) << 16 | u32::from(rgb[1]) << 8 | u32::from(rgb[2]))
}
fn used_colors(project: &Project) -> UsedColors {
    let mut colors = BTreeMap::<String, Vec<u32>>::new();
    let mut add = |tag: String, color: &Value| {
        if let Some(color) = color_value(color) {
            let list = colors.entry(tag).or_default();
            if !list.contains(&color) {
                list.push(color);
            }
        }
    };
    add("bg".into(), &project.json["bgColor"]);
    for key in project.level_keys() {
        if let Some(level) = project.level(key) {
            add("bg".into(), &level["bgColor"]);
            for (prefix, fields) in std::iter::once(("l_", &level["fieldInstances"])).chain(
                level["layerInstances"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .flat_map(|layer| layer["entityInstances"].as_array().into_iter().flatten())
                    .map(|entity| ("e_", &entity["fieldInstances"])),
            ) {
                for field in fields.as_array().into_iter().flatten() {
                    if field["__type"]
                        .as_str()
                        .is_some_and(|kind| kind == "Color" || kind == "Array<Color>")
                    {
                        let tag = format!("{prefix}{}", text(field, "__identifier", ""));
                        if let Some(values) = field["__value"].as_array() {
                            for value in values {
                                add(tag.clone(), value);
                            }
                        } else {
                            add(tag, &field["__value"]);
                        }
                    }
                }
            }
        }
    }
    for values in colors.values_mut() {
        values.sort_by(|a, b| rgb_hsv(rgb(*a))[0].total_cmp(&rgb_hsv(rgb(*b))[0]));
    }
    UsedColors(colors)
}
impl EditorApp {
    pub(super) fn prepare_color_context(&mut self, ctx: &egui::Context) {
        ui_fields::prepare_dialog_context(self, ctx);
        if let Some(message) =
            ctx.data_mut(|d| d.remove_temp::<&'static str>(active_color_id().with("notice")))
        {
            if message == "Pasted color" {
                self.quick_notification =
                    Some((locale::tr_ctx(ctx, message).into(), None, Instant::now()));
                ctx.request_repaint();
            } else {
                self.status = message.into();
                self.status_error = false;
            }
        }
        let key = egui::Id::new("source-show-project-colors");
        if let Some(value) = ctx.data_mut(|d| d.remove_temp::<bool>(key.with("changed"))) {
            self.settings
                .set_ui_state_int("ShowProjectColors", None, i64::from(value));
        }
        ctx.data_mut(|d| {
            d.insert_temp(
                key,
                self.settings.ui_state_int("ShowProjectColors", None, 1) != 0,
            );
            if let Some(project) = &self.project {
                d.insert_temp(key.with("used"), used_colors(project));
            }
        });
    }
}

fn source_hex(input: &str) -> Option<[u8; 3]> {
    let input = input.trim().trim_start_matches('#');
    let expanded = if input.len() >= 6 && input.as_bytes()[..6].iter().all(u8::is_ascii_hexdigit) {
        input[..6].to_owned()
    } else if input.len() == 3 && input.bytes().all(|c| c.is_ascii_hexdigit()) {
        input.chars().flat_map(|c| [c, c]).collect()
    } else if input.len() == 1 && input.bytes().all(|c| c.is_ascii_hexdigit()) {
        input.repeat(6)
    } else {
        return None;
    };
    u32::from_str_radix(&expanded, 16).ok().map(rgb)
}
fn hsv_rgb([h, s, v]: [f64; 3]) -> [u8; 3] {
    let h = h * 6.;
    let i = h.floor();
    let f = h - i;
    let p = v * (1. - s);
    let q = v * (1. - f * s);
    let t = v * (1. - (1. - f) * s);
    let i = i as usize % 6;
    [
        [v, q, p, p, t, v][i],
        [t, v, v, q, p, p][i],
        [p, p, t, v, v, q][i],
    ]
    .map(|c| (c * 255.).round().clamp(0., 255.) as u8)
}
fn rgb_hsv(rgb: [u8; 3]) -> [f64; 3] {
    let [r, g, b] = rgb.map(|c| f64::from(c) / 255.);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let d = max - min;
    let h = if d == 0. {
        0.
    } else if max == r {
        ((g - b) / d + if g < b { 6. } else { 0. }) / 6.
    } else if max == g {
        ((b - r) / d + 2.) / 6.
    } else {
        ((r - g) / d + 4.) / 6.
    };
    [h, if max == 0. { 0. } else { d / max }, max]
}
fn hex(rgb: [u8; 3]) -> String {
    format!("{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2])
}
#[derive(Clone)]
struct ColorDraft {
    rgb: [u8; 3],
    hsv: [f64; 3],
    hex: String,
    copied: bool,
    paste_color: Option<[u8; 3]>,
    clipboard_frame: Option<u64>,
    show_all: bool,
    tag: Option<String>,
    nullified: bool,
}
impl ColorDraft {
    fn new(rgb: [u8; 3]) -> Self {
        Self {
            rgb,
            hsv: rgb_hsv(rgb),
            hex: hex(rgb),
            copied: false,
            paste_color: None,
            clipboard_frame: None,
            show_all: false,
            tag: None,
            nullified: false,
        }
    }
    fn assign(&mut self, rgb: [u8; 3]) {
        self.nullified = false;
        self.rgb = rgb;
        self.hsv = rgb_hsv(rgb);
        self.hex = hex(rgb);
        self.copied = false;
    }
}
fn active_color_id() -> egui::Id {
    egui::Id::new("source-active-color-picker")
}
pub(in crate::app) fn picker_is_open(ctx: &egui::Context) -> bool {
    ctx.data_mut(|d| {
        d.get_temp::<egui::Id>(active_color_id())
            .and_then(|id| d.get_temp::<ColorDraft>(id))
    })
    .is_some()
}
pub(in crate::app) fn picker_rect(ctx: &egui::Context) -> Option<Rect> {
    ctx.data_mut(|d| d.get_temp::<Rect>(active_color_id().with("rect")))
}
pub(in crate::app) fn request_validate(ctx: &egui::Context) {
    if picker_is_open(ctx) {
        let frame = ctx.cumulative_frame_nr() + 1;
        ctx.data_mut(|d| d.insert_temp(active_color_id().with("validate"), frame));
        ctx.request_repaint();
    }
}
#[derive(Clone, Copy)]
struct ClipboardRead {
    picker: egui::Id,
    frame: u64,
    apply: bool,
}

fn clipboard_reads_id() -> egui::Id {
    active_color_id().with("clipboard-reads")
}

fn read_clipboard(ctx: &egui::Context, id: egui::Id, state: &mut ColorDraft, apply: bool) {
    // As with the rule menu's preview, eframe puts the result in raw input two frames later.
    let frame = ctx.cumulative_frame_nr() + 2;
    state.clipboard_frame = Some(frame);
    let read = ClipboardRead {
        picker: id,
        frame,
        apply,
    };
    let queued = ctx.data_mut(|d| {
        let reads = d.get_temp_mut_or_default::<Vec<ClipboardRead>>(clipboard_reads_id());
        if let Some(existing) = reads.iter_mut().find(|existing| existing.frame == frame) {
            *existing = read;
            false
        } else {
            reads.push(read);
            true
        }
    });
    if queued {
        ctx.send_viewport_cmd(egui::ViewportCommand::RequestPaste);
    }
    ctx.request_repaint();
}

pub(in crate::app) fn capture_color_clipboard(ctx: &egui::Context, input: &mut egui::RawInput) {
    let frame = ctx.cumulative_frame_nr();
    ctx.data_mut(|d| {
        let Some(mut reads) = d.get_temp::<Vec<ClipboardRead>>(clipboard_reads_id()) else {
            return;
        };
        let read = reads.iter().find(|read| read.frame == frame).copied();
        reads.retain(|read| read.frame > frame);
        if reads.is_empty() {
            d.remove::<Vec<ClipboardRead>>(clipboard_reads_id());
        } else {
            d.insert_temp(clipboard_reads_id(), reads);
        }
        let Some(read) = read else { return };
        let color = if let Some(egui::Event::Paste(raw)) = input.events.first() {
            let color = source_hex(raw);
            input.events.remove(0);
            color
        } else {
            None
        };
        if d.get_temp::<egui::Id>(active_color_id()) != Some(read.picker) {
            return;
        }
        let Some(mut draft) = d.get_temp::<ColorDraft>(read.picker) else {
            return;
        };
        if draft.clipboard_frame != Some(frame) {
            return;
        }
        draft.clipboard_frame = None;
        if read.apply {
            if let Some(color) = color {
                // The source button updates the preview. The HEX field refreshes on blur.
                let field = std::mem::take(&mut draft.hex);
                draft.assign(color);
                draft.hex = field;
                d.insert_temp(active_color_id().with("notice"), "Pasted color");
            }
        } else {
            draft.paste_color = color;
        }
        d.insert_temp(read.picker, draft);
    });
}
fn spectrum(ui: &mut egui::Ui, state: &mut ColorDraft) {
    let (whole, _) = ui.allocate_exact_size(egui::vec2(320., 150.), Sense::hover());
    let plane = Rect::from_min_size(whole.min, egui::vec2(295., 150.));
    let hue = Rect::from_min_size(whole.min + egui::vec2(300., 0.), egui::vec2(20., 150.));
    let response = ui.interact(
        plane,
        ui.id().with("saturation-brightness"),
        Sense::click_and_drag(),
    );
    let hue_response = ui.interact(hue, ui.id().with("hue"), Sense::click_and_drag());
    let old = state.hsv;
    if (response.clicked() || response.dragged())
        && let Some(pos) = response.interact_pointer_pos()
    {
        state.hsv[1] = f64::from(((pos.x - plane.left()) / 295.).clamp(0., 1.));
        state.hsv[2] = f64::from((1. - (pos.y - plane.top()) / 150.).clamp(0., 1.));
    }
    if (hue_response.clicked() || hue_response.dragged())
        && let Some(pos) = hue_response.interact_pointer_pos()
    {
        state.hsv[0] = f64::from((1. - (pos.y - hue.top()).clamp(0., 148.) / 148.).clamp(0., 1.));
    }
    if state.hsv != old {
        state.rgb = hsv_rgb(state.hsv);
        state.hex = hex(state.rgb);
        state.copied = false;
    }
    let full = hsv_rgb([state.hsv[0], 1., 1.]);
    let saturated = Color32::from_rgb(full[0], full[1], full[2]);
    let mut mesh = egui::Mesh::default();
    for (pos, col) in [
        (plane.left_top(), Color32::WHITE),
        (plane.right_top(), saturated),
        (plane.left_bottom(), Color32::WHITE),
        (plane.right_bottom(), saturated),
        (plane.left_top(), Color32::TRANSPARENT),
        (plane.right_top(), Color32::TRANSPARENT),
        (plane.left_bottom(), Color32::BLACK),
        (plane.right_bottom(), Color32::BLACK),
    ] {
        mesh.colored_vertex(pos, col);
    }
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(1, 2, 3);
    // Value darkens the entire white-to-hue ramp by the same factor at each height.
    mesh.add_triangle(4, 5, 6);
    mesh.add_triangle(5, 6, 7);
    ui.painter().add(mesh);
    let colors = [
        0xff0000, 0xff00ff, 0x0000ff, 0x00ffff, 0x00ff00, 0xffff00, 0xff0000,
    ];
    let stops = [0., 0.17, 0.34, 0.5, 0.67, 0.84, 1.];
    let mut mesh = egui::Mesh::default();
    for i in 0..6 {
        let top = hue.top() + 150. * stops[i];
        let bottom = hue.top() + 150. * stops[i + 1];
        let a = rgb(colors[i]);
        let b = rgb(colors[i + 1]);
        let base = mesh.vertices.len() as u32;
        for (pos, c) in [
            (egui::pos2(hue.left(), top), a),
            (egui::pos2(hue.right(), top), a),
            (egui::pos2(hue.left(), bottom), b),
            (egui::pos2(hue.right(), bottom), b),
        ] {
            mesh.colored_vertex(pos, Color32::from_rgb(c[0], c[1], c[2]));
        }
        mesh.add_triangle(base, base + 1, base + 2);
        mesh.add_triangle(base + 1, base + 2, base + 3);
    }
    ui.painter().add(mesh);
    let selected = plane.min
        + egui::vec2(
            state.hsv[1] as f32 * 295.,
            (1. - state.hsv[2]) as f32 * 150.,
        );
    let [r, g, b] = state.rgb.map(f32::from);
    let marker = if 0.299 * r + 0.587 * g + 0.114 * b < 128. {
        Color32::WHITE
    } else {
        Color32::BLACK
    };
    ui.painter()
        .circle_stroke(selected, 6., Stroke::new(2., marker));
    let y = hue.top() + (1. - state.hsv[0]) as f32 * 148.;
    ui.painter().rect_filled(
        Rect::from_min_size(egui::pos2(hue.right() - 7., y), egui::vec2(10., 2.)),
        0.,
        Color32::WHITE,
    );
}

fn color_palette(ui: &mut egui::Ui, id: egui::Id, draft: &mut ColorDraft) {
    let blind = ui
        .data(|d| d.get_temp::<bool>(egui::Id::new("source-colorblind-palette")))
        .unwrap_or(false);
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 0.;
        let mut choices = palette(blind).to_vec();
        if let Some(tag) = &draft.tag {
            let key = egui::Id::new("source-show-project-colors");
            let mut expanded = ui.data(|d| d.get_temp::<bool>(key)).unwrap_or(true);
            let label = if draft.show_all {
                "All colors used in this project"
            } else {
                "Colors used for this element"
            };
            let (rect, response) = ui.allocate_exact_size(egui::vec2(320., 24.), Sense::click());
            let label = locale::tr(ui, label);
            ui.painter().rect_filled(
                rect,
                0.,
                if expanded {
                    theme::LIGHT
                } else {
                    theme::MEDIUM
                },
            );
            egui::Image::new(if expanded {
                egui::include_image!("../../assets/icons/expanded.svg")
            } else {
                egui::include_image!("../../assets/icons/collapsed.svg")
            })
            .paint_at(
                ui,
                Rect::from_center_size(rect.left_center() + egui::vec2(10., 0.), Vec2::splat(18.)),
            );
            ui.painter().text(
                rect.left_center() + egui::vec2(22., 0.),
                Align2::LEFT_CENTER,
                label,
                FontId::proportional(11.733),
                if response.hovered() {
                    theme::ACCENT
                } else if expanded {
                    Color32::WHITE
                } else {
                    theme::TEXT
                },
            );
            response
                .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label));
            if response
                .on_hover_cursor(if expanded {
                    egui::CursorIcon::PointingHand
                } else {
                    egui::CursorIcon::Cell
                })
                .clicked()
            {
                expanded = !expanded;
                ui.data_mut(|d| {
                    d.insert_temp(key, expanded);
                    d.insert_temp(key.with("changed"), expanded);
                });
            }
            if !expanded {
                return;
            }
            let used = ui
                .data(|d| d.get_temp::<UsedColors>(key.with("used")))
                .unwrap_or_default();
            choices = if draft.show_all {
                let mut all: Vec<_> = used.0.into_values().flatten().collect();
                all.sort_by(|a, b| rgb_hsv(rgb(*a))[0].total_cmp(&rgb_hsv(rgb(*b))[0]));
                all
            } else {
                used.0.get(tag).cloned().unwrap_or_default()
            };
        }

        let panel = egui::Frame::new()
            .fill(theme::MEDIUM)
            .corner_radius(egui::CornerRadius {
                nw: 0,
                ne: 0,
                sw: 4,
                se: 4,
            })
            .inner_margin(egui::Margin {
                left: 5,
                right: 4,
                top: 4,
                bottom: 4,
            })
            .show(ui, |ui| {
                ui.set_width(311.);
                if choices.is_empty() {
                    ui.label(
                        RichText::new(locale::tr(ui, "Empty"))
                            .italics()
                            .color(theme::TEXT),
                    );
                    return;
                }
                let show_all_link = draft.tag.is_some() && !draft.show_all;
                let height = 24 * (choices.len().div_ceil(12) + usize::from(show_all_link));
                let mut render = |ui: &mut egui::Ui| {
                    let width = ui.available_width() / 12.;
                    egui::Grid::new(id.with("palette"))
                        .min_col_width(0.)
                        .spacing(Vec2::ZERO)
                        .show(ui, |ui| {
                            for (index, &choice) in choices.iter().enumerate() {
                                let [r, g, b] = rgb(choice);
                                let (rect, response) =
                                    ui.allocate_exact_size(egui::vec2(width, 24.), Sense::click());
                                ui.painter().rect(
                                    rect,
                                    0.,
                                    Color32::from_rgb(r, g, b),
                                    if response.hovered() {
                                        Stroke::new(2., Color32::WHITE)
                                    } else {
                                        Stroke::new(1., Color32::BLACK)
                                    },
                                    StrokeKind::Inside,
                                );
                                if response.on_hover_text(format!("#{choice:06X}")).clicked() {
                                    draft.assign([r, g, b]);
                                }
                                if index % 12 == 11 {
                                    ui.end_row();
                                }
                            }
                        });
                    if show_all_link {
                        let label = locale::tr(ui, "Show all colors used in this project");
                        if ui
                            .add(egui::Link::new(
                                RichText::new(label).size(11.733).color(theme::ACCENT),
                            ))
                            .clicked()
                        {
                            draft.show_all = true;
                        }
                    }
                };
                if height > 180 {
                    egui::ScrollArea::vertical()
                        .id_salt(id.with("palette-scroll"))
                        .max_height(180.)
                        .show(ui, render);
                } else {
                    render(ui);
                }
            });
        let rect = panel.response.rect;
        ui.painter().vline(
            rect.left() + 0.5,
            rect.y_range(),
            Stroke::new(1., theme::LIGHT),
        );
    });
}

/// Original RGB-only picker: SV plane, reversed hue bar, hexadecimal input, clipboard and palette.
/// Draft choices validate on Enter, Escape or click outside; they do not create per-drag undo entries.
pub(in crate::app) fn color_editor(ui: &mut egui::Ui, color: &mut [u8; 3]) -> egui::Response {
    color_editor_tag(ui, color, None)
}
pub(in crate::app) fn color_editor_tag(
    ui: &mut egui::Ui,
    color: &mut [u8; 3],
    tag: Option<&str>,
) -> egui::Response {
    color_editor_sized(ui, color, tag, egui::vec2(38., 24.))
}

pub(super) fn color_editor_sized(
    ui: &mut egui::Ui,
    color: &mut [u8; 3],
    tag: Option<&str>,
    size: Vec2,
) -> egui::Response {
    let response = source_controls::add_sized_button(
        ui,
        size,
        egui::Button::new(" ")
            .fill(Color32::from_rgb(color[0], color[1], color[2]))
            .stroke(Stroke::new(1_f32, Color32::GRAY)),
    );
    color_picker(ui, color, tag, response, false, None).0
}
fn color_picker(
    ui: &mut egui::Ui,
    color: &mut [u8; 3],
    tag: Option<&str>,
    mut response: egui::Response,
    force_open: bool,
    initial_null: Option<bool>,
) -> (egui::Response, Option<bool>) {
    let allow_null = initial_null.is_some();
    let mut validated = None;
    let id = response.id.with("source-color-draft");
    let mut state = ui.ctx().data_mut(|d| d.get_temp::<ColorDraft>(id));
    let just_opened = (response.clicked() || force_open) && state.is_none();
    if just_opened {
        let mut draft = ColorDraft::new(*color);
        draft.tag = tag.map(str::to_owned);
        draft.nullified = initial_null.unwrap_or(false);
        if force_open && draft.nullified {
            draft.hex.clear();
        }
        read_clipboard(ui.ctx(), id, &mut draft, false);
        state = Some(draft);
    }
    if ui.ctx().data(|d| {
        d.get_temp::<Vec<ClipboardRead>>(clipboard_reads_id())
            .is_some()
    }) {
        ui.ctx().request_repaint();
    }
    if let Some(mut draft) = state {
        ui.ctx().data_mut(|d| d.insert_temp(active_color_id(), id));
        let mut open = !ui
            .ctx()
            .data_mut(|d| d.get_temp::<u64>(active_color_id().with("validate")))
            .is_some_and(|frame| ui.ctx().cumulative_frame_nr() >= frame);
        let screen = ui.ctx().content_rect();
        egui::Area::new(id.with("backdrop"))
            .order(egui::Order::Foreground)
            .fixed_pos(screen.min)
            .interactable(false)
            .constrain(false)
            .show(ui.ctx(), |ui| {
                ui.painter()
                    .rect_filled(screen, 0., Color32::from_black_alpha(128));
            });
        let popup = egui::Popup::menu(&response)
            .open_bool(&mut open)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .frame(
                egui::Frame::new()
                    .fill(theme::DARK)
                    .stroke(Stroke::new(1_f32, theme::MEDIUM))
                    .inner_margin(4)
                    .corner_radius(3),
            )
            .show(|ui| {
                // Consume the HEX cursor's scroll target here, before the owner's form does.
                egui::ScrollArea::new([false, false])
                    .id_salt(id.with("focus-scroll"))
                    .show(ui, |ui| {
                        ui.set_width(320.);
                        ui.spacing_mut().item_spacing = egui::vec2(4., 4.);
                        ui.spacing_mut().button_padding = Vec2::ZERO;
                        let (rect, _) =
                            ui.allocate_exact_size(egui::vec2(320., 32.), Sense::hover());
                        ui.painter().rect_filled(
                            rect,
                            0.,
                            Color32::from_rgb(draft.rgb[0], draft.rgb[1], draft.rgb[2]),
                        );
                        ui.painter().rect_stroke(
                            rect,
                            0.,
                            Stroke::new(1_f32, theme::LIGHT),
                            StrokeKind::Inside,
                        );
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 2.;
                            ui.visuals_mut().selection.bg_fill = Color32::from_rgb(0, 102, 204);
                            ui.visuals_mut().selection.stroke.color = Color32::WHITE;
                            let mut text = draft.hex.clone();
                            let field = ui.add_sized(
                                [if allow_null { 182. } else { 228. }, 44.],
                                egui::TextEdit::singleline(&mut text)
                                    .hint_text("rrggbb")
                                    .font(FontId::proportional(13.333))
                                    .text_color(Color32::WHITE)
                                    .vertical_align(egui::Align::Center)
                                    .margin(egui::Margin {
                                        left: 17,
                                        right: 4,
                                        top: 0,
                                        bottom: 0,
                                    }),
                            );
                            if just_opened {
                                field.request_focus();
                                if let Some(mut state) =
                                    egui::text_edit::TextEditState::load(ui.ctx(), field.id)
                                {
                                    state.cursor.set_char_range(Some(
                                        egui::text::CCursorRange::two(
                                            egui::text::CCursor::new(0),
                                            egui::text::CCursor::new(draft.hex.len()),
                                        ),
                                    ));
                                    state.store(ui.ctx(), field.id);
                                }
                            }
                            ui.painter().rect_stroke(
                                field.rect,
                                0.,
                                Stroke::new(
                                    1.,
                                    if field.has_focus() {
                                        theme::ACCENT
                                    } else {
                                        theme::LIGHT
                                    },
                                ),
                                StrokeKind::Inside,
                            );
                            ui.painter().text(
                                field.rect.left_center() + egui::vec2(7., 0.),
                                Align2::LEFT_CENTER,
                                "#",
                                FontId::proportional(14.),
                                theme::LIGHT,
                            );
                            if field.changed() {
                                text = text.replace('#', "");
                                if let Some(rgb) = source_hex(&text) {
                                    let original = text.clone();
                                    draft.assign(rgb);
                                    draft.hex = original;
                                } else {
                                    draft.hex = text;
                                }
                            }
                            if field.lost_focus() {
                                draft.hex = hex(draft.rgb);
                            }
                            if source_controls::add_sized_button(
                                ui,
                                [44., 44.],
                                egui::Button::image(
                                    egui::Image::new(egui::include_image!(
                                        "../../assets/icons/copy.svg"
                                    ))
                                    .tint(if draft.copied {
                                        Color32::from_white_alpha(51)
                                    } else {
                                        Color32::WHITE
                                    })
                                    .fit_to_exact_size(Vec2::splat(24.)),
                                )
                                .fill(if draft.copied {
                                    theme::DARK
                                } else {
                                    theme::LIGHT
                                }),
                            )
                            .on_hover_text(locale::tr(ui, "Copy color"))
                            .clicked()
                            {
                                ui.ctx().copy_text(format!("#{}", hex(draft.rgb)));
                                draft.copied = true;
                                draft.paste_color = Some(draft.rgb);
                                draft.clipboard_frame = None;
                                ui.data_mut(|d| {
                                    d.insert_temp(
                                        active_color_id().with("notice"),
                                        "Copied to clipboard.",
                                    )
                                });
                            }
                            if source_controls::add_sized_button(
                                ui,
                                [44., 44.],
                                egui::Button::image(
                                    egui::Image::new(egui::include_image!(
                                        "../../assets/icons/paste.svg"
                                    ))
                                    .tint(if draft.paste_color.is_some() {
                                        Color32::WHITE
                                    } else {
                                        Color32::from_white_alpha(38)
                                    })
                                    .fit_to_exact_size(Vec2::splat(24.)),
                                )
                                .fill(
                                    draft.paste_color.map_or(theme::DARK, |[r, g, b]| {
                                        Color32::from_rgb(r, g, b)
                                    }),
                                ),
                            )
                            .on_hover_text(locale::tr(ui, "Paste color"))
                            .clicked()
                            {
                                read_clipboard(ui.ctx(), id, &mut draft, true);
                            }
                            if allow_null
                                && source_controls::add_sized_button(
                                    ui,
                                    [44., 44.],
                                    egui::Button::image(
                                        egui::Image::new(egui::include_image!(
                                            "../../assets/icons/delete.svg"
                                        ))
                                        .fit_to_exact_size(Vec2::splat(24.)),
                                    )
                                    .fill(Color32::from_rgb(121, 17, 5)),
                                )
                                .on_hover_text(locale::tr(ui, "Remove color"))
                                .clicked()
                            {
                                draft.nullified = true;
                                ui.close();
                            }
                        });
                        spectrum(ui, &mut draft);
                        color_palette(ui, id, &mut draft);
                        if ui.input(|i| i.key_pressed(Key::Enter)) {
                            ui.close();
                        }
                    });
            });
        if let Some(popup) = popup {
            ui.ctx()
                .data_mut(|d| d.insert_temp(active_color_id().with("rect"), popup.response.rect));
        }
        if open {
            ui.ctx().data_mut(|d| d.insert_temp(id, draft));
        } else {
            validated = Some(draft.nullified);
            if *color != draft.rgb {
                *color = draft.rgb;
                response.mark_changed();
            }
            ui.ctx().data_mut(|d| {
                d.remove::<ColorDraft>(id);
                d.remove::<egui::Id>(active_color_id());
                d.remove::<Rect>(active_color_id().with("rect"));
                d.remove::<u64>(active_color_id().with("validate"));
            });
        }
    }
    (
        response.on_hover_text(format!("#{}", hex(*color))),
        validated,
    )
}
pub(in crate::app) fn nullable_color_editor(
    ui: &mut egui::Ui,
    color: &mut Option<[u8; 3]>,
) -> egui::Response {
    nullable_color_editor_sized(ui, color, None, egui::vec2(38., 24.))
}

/// Form color input with the original swatch and a separate reset control.
pub(super) fn form_color_editor(ui: &mut egui::Ui, color: &mut Option<[u8; 3]>) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(64., 32.), Sense::click());
    let mut rgb = color.unwrap_or([0, 0, 0]);
    let mut inner = Color32::from_rgb(rgb[0], rgb[1], rgb[2]);
    let mut border = Color32::from_gray(119);
    let painter = ui.painter();
    if color.is_some() {
        painter.rect_filled(rect, 2., Color32::from_gray(118));
        painter.rect_filled(rect.shrink(1.), 1., Color32::from_gray(239));
    } else {
        let opacity = if response.hovered() { 255 } else { 77 };
        painter.rect_filled(
            rect,
            0.,
            if response.hovered() {
                Color32::GRAY
            } else {
                Color32::from_rgb(71, 75, 83)
            },
        );
        egui::Image::new(egui::include_image!("../../assets/img/stripes.png"))
            .texture_options(egui::TextureOptions::NEAREST_REPEAT)
            .tint(Color32::from_white_alpha(opacity))
            .uv(Rect::from_min_size(egui::Pos2::ZERO, rect.size() / 8.))
            .paint_at(ui, rect);
        if !response.hovered() {
            inner = Color32::from_rgb(32, 36, 44);
            border = Color32::from_rgb(68, 72, 80);
        }
    }
    painter.rect_filled(rect.shrink2(egui::vec2(3., 5.)), 0., border);
    painter.rect_filled(rect.shrink2(egui::vec2(4., 6.)), 0., inner);
    if response.hovered() {
        painter.rect_stroke(
            rect,
            0.,
            Stroke::new(1., theme::ACCENT),
            StrokeKind::Outside,
        );
    }
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, true, format!("#{}", hex(rgb)))
    });
    let (mut response, validated) = color_picker(
        ui,
        &mut rgb,
        None,
        response.on_hover_cursor(egui::CursorIcon::PointingHand),
        false,
        None,
    );
    if validated.is_some() && *color != Some(rgb) {
        *color = Some(rgb);
        response.mark_changed();
    }
    response
}

pub(super) fn inherited_color_editor(
    ui: &mut egui::Ui,
    color: &mut Option<[u8; 3]>,
    default: Color32,
) -> egui::Response {
    nullable_color_editor_sized(ui, color, Some(default), egui::vec2(62., 32.))
}

fn nullable_color_editor_sized(
    ui: &mut egui::Ui,
    color: &mut Option<[u8; 3]>,
    default: Option<Color32>,
    size: Vec2,
) -> egui::Response {
    let inherited = default.is_some() && color.is_none();
    let background = color
        .map(|[r, g, b]| Color32::from_rgb(r, g, b))
        .or(default)
        .unwrap_or(Color32::BLACK);
    let mut rgb = [background.r(), background.g(), background.b()];
    let response = source_controls::add_sized_button(
        ui,
        size,
        egui::Button::new(" ")
            .fill(background)
            .stroke(Stroke::new(1_f32, Color32::GRAY)),
    );
    if inherited {
        source_controls::disabled_hatch(ui, response.rect.shrink(2.));
    }
    if default.is_some() {
        let icon = egui::Image::new(egui::include_image!("../../assets/icons/color.svg"))
            .tint(Color32::from_white_alpha(180));
        icon.paint_at(
            ui,
            Rect::from_center_size(response.rect.center(), Vec2::splat(19.)),
        );
    }
    let (mut response, validation) =
        color_picker(ui, &mut rgb, None, response, false, Some(inherited));
    if let Some(null) = validation {
        let next = if null { None } else { Some(rgb) };
        if *color != next {
            *color = next;
            response.mark_changed();
        }
    }
    response
}
/// Context menus open the picker directly, without a swatch click.
pub(in crate::app) fn standalone_color_dialog(
    ctx: &egui::Context,
    id: egui::Id,
    color: &mut [u8; 3],
    first_frame: bool,
) -> bool {
    standalone_color_picker(ctx, id, color, first_frame, None);
    !picker_is_open(ctx)
}

pub(super) fn standalone_nullable_color_dialog(
    ctx: &egui::Context,
    id: egui::Id,
    color: &mut Option<[u8; 3]>,
    first_frame: bool,
) -> bool {
    let mut rgb = color.unwrap_or([0; 3]);
    if let Some(null) =
        standalone_color_picker(ctx, id, &mut rgb, first_frame, Some(color.is_none()))
    {
        *color = (!null).then_some(rgb);
    }
    !picker_is_open(ctx)
}

fn standalone_color_picker(
    ctx: &egui::Context,
    id: egui::Id,
    color: &mut [u8; 3],
    first_frame: bool,
    initial_null: Option<bool>,
) -> Option<bool> {
    egui::Area::new(id)
        .order(egui::Order::Foreground)
        .fixed_pos(ctx.content_rect().center() - egui::vec2(160., 160.))
        .show(ctx, |ui| {
            let (_, response) = ui.allocate_exact_size(Vec2::ZERO, Sense::hover());
            color_picker(ui, color, None, response, first_frame, initial_null).1
        })
        .inner
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn popup_is_compact_and_hex_focus_does_not_scroll_parent_form() {
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        ctx.style_mut(|s| s.animation_time = 0.);
        let mut color = [0, 255, 0];
        let swatch = std::cell::Cell::new(Rect::NOTHING);
        let offset = std::cell::Cell::new(0.);
        let run = |events: Vec<egui::Event>, color: &mut [u8; 3]| {
            ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1000., 800.),
                    )),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        let output = egui::ScrollArea::vertical()
                            .id_salt("parent-color-form")
                            .show(ui, |ui| {
                                ui.add_space(250.);
                                swatch.set(color_editor(ui, color).rect);
                                ui.add_space(1500.);
                            });
                        offset.set(output.state.offset.y);
                    });
                },
            )
        };
        run(vec![], &mut color);
        let pos = swatch.get().center();
        let mut clipboard_reads = 0;
        for pressed in [true, false] {
            let output = run(
                vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: Default::default(),
                    },
                ],
                &mut color,
            );
            clipboard_reads += output
                .viewport_output
                .values()
                .flat_map(|viewport| &viewport.commands)
                .filter(|command| matches!(command, egui::ViewportCommand::RequestPaste))
                .count();
        }
        for _ in 0..4 {
            run(vec![], &mut color);
        }
        let rect = picker_rect(&ctx).expect("open picker");
        assert_eq!(
            clipboard_reads, 1,
            "opening reads the paste button's color preview"
        );
        assert!(
            (rect.width() - 330.).abs() <= 2.,
            "source320pt content plus4pt margins/border: {rect:?}"
        );
        assert_eq!(
            offset.get(),
            0.,
            "HEX autofocus must only scroll within the popup"
        );
        assert!(swatch.get().top() > 200.);
    }
    #[test]
    fn requested_clipboard_paste_uses_source_hex_without_triggering_definition_paste() {
        let ctx = egui::Context::default();
        let id = egui::Id::new("clipboard-color-test");
        ctx.data_mut(|d| {
            d.insert_temp(active_color_id(), id);
            d.insert_temp(id, ColorDraft::new([0, 0, 0]));
        });
        let fetch = |apply, clipboard: Option<&str>| {
            let mut state = ctx.data(|d| d.get_temp::<ColorDraft>(id)).unwrap();
            read_clipboard(&ctx, id, &mut state, apply);
            ctx.data_mut(|d| d.insert_temp(id, state));
            for _ in 0..2 {
                let _ = ctx.run(egui::RawInput::default(), |_| {});
            }
            let events = clipboard
                .into_iter()
                .map(|raw| egui::Event::Paste(raw.into()))
                .chain(std::iter::once(egui::Event::Text("ordinary input".into())))
                .collect();
            let mut input = egui::RawInput {
                events,
                ..Default::default()
            };
            capture_color_clipboard(&ctx, &mut input);
            assert_eq!(input.events, [egui::Event::Text("ordinary input".into())]);
            ctx.data(|d| d.get_temp::<ColorDraft>(id)).unwrap()
        };
        let preview = fetch(false, Some("#0af"));
        assert_eq!(preview.paste_color, Some([0, 170, 255]));
        assert_eq!(preview.rgb, [0, 0, 0]);
        let paste = fetch(true, Some("#3377CC"));
        assert_eq!(paste.rgb, [51, 119, 204]);
        assert_eq!(paste.hex, "000000");
        assert_eq!(fetch(true, None).rgb, paste.rgb);
        assert_eq!(fetch(false, Some("not a color")).paste_color, None);

        let mut ordinary = egui::RawInput {
            events: vec![egui::Event::Paste("ordinary paste".into())],
            ..Default::default()
        };
        capture_color_clipboard(&ctx, &mut ordinary);
        assert_eq!(
            ordinary.events,
            [egui::Event::Paste("ordinary paste".into())]
        );
    }
    #[test]
    fn nullable_group_color_delete_clears_without_converting_to_black() {
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        ctx.style_mut(|s| s.animation_time = 0.);
        let mut color = Some([10, 20, 30]);
        let rect = std::cell::Cell::new(Rect::NOTHING);
        let run = |events: Vec<egui::Event>, color: &mut Option<[u8; 3]>| {
            ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(900., 800.),
                    )),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        rect.set(nullable_color_editor(ui, color).rect);
                    });
                },
            )
        };
        run(vec![], &mut color);
        let pos = rect.get().center();
        for pressed in [true, false] {
            run(
                vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: Default::default(),
                    },
                ],
                &mut color,
            );
        }
        let output = run(vec![], &mut color);
        let pos = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Rect(rect) if rect.fill == Color32::from_rgb(121, 17, 5) => {
                    Some(rect.rect.center())
                }
                _ => None,
            })
            .expect("source red remove-color button");
        for pressed in [true, false] {
            run(
                vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: Default::default(),
                    },
                ],
                &mut color,
            );
        }
        assert_eq!(color, None);
    }
    #[test]
    fn source_hex_inputs_expand_shorthand_and_reject_invalid_values() {
        for (input, expected) in [
            ("#f", Some([255, 255, 255])),
            ("  ##1aF  ", Some([17, 170, 255])),
            ("#12345678", Some([18, 52, 86])),
            ("0x123456", None),
            ("12", None),
            ("#12g", None),
            ("#12345", None),
        ] {
            assert_eq!(source_hex(input), expected, "{input}");
        }
        assert_eq!(hsv_rgb([0., 1., 1.]), [255, 0, 0]);
        assert_eq!(hsv_rgb([0.5, 1., 1.]), [0, 255, 255]);
        assert_eq!(hsv_rgb([1., 0.5, 0.5]), [128, 64, 64]);
    }
    #[test]
    fn spectrum_pointer_updates_draft_and_escape_commits_once() {
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        ctx.style_mut(|s| s.animation_time = 0.);
        let mut color = [0, 255, 0];
        let button = std::cell::Cell::new(Rect::NOTHING);
        let changed = std::cell::Cell::new(false);
        let run = |events: Vec<egui::Event>, color: &mut [u8; 3]| {
            ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1000., 800.),
                    )),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        let response = color_editor(ui, color);
                        button.set(response.rect);
                        changed.set(response.changed());
                    });
                },
            )
        };
        run(vec![], &mut color);
        let pos = button.get().center();
        for pressed in [true, false] {
            run(
                vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: Default::default(),
                    },
                ],
                &mut color,
            );
        }
        let output = run(vec![], &mut color);
        let plane = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Mesh(mesh) => {
                    let mut rect = Rect::NOTHING;
                    for vertex in &mesh.vertices {
                        rect.extend_with(vertex.pos);
                    }
                    ((rect.width() - 295.).abs() < 0.01 && (rect.height() - 150.).abs() < 0.01)
                        .then_some(rect)
                }
                _ => None,
            })
            .expect("source 295x150 saturation/brightness plane");
        let pos = plane.left_top();
        for pressed in [true, false] {
            run(
                vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: Default::default(),
                    },
                ],
                &mut color,
            );
        }
        assert_eq!(color, [0, 255, 0], "source waits until picker validation");
        run(
            vec![egui::Event::Key {
                key: Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Default::default(),
            }],
            &mut color,
        );
        assert_eq!(color, [255, 255, 255]);
        assert!(changed.get());
    }
    #[test]
    fn original_palette_order_distance_and_least_used_match_compiled_source() {
        assert_eq!(palette(false).len(), 32);
        assert_eq!(palette(true).len(), 16);
        assert_eq!(suggest(false, &[]), 0xbe4a2f);
        assert_eq!(suggest(true, &[]), 0);
        assert_eq!(suggest(false, &[0xbe4a2f]), 0xd77643);
        assert_eq!(suggest(true, &[0, 0x252525]), 0x676767);
        assert!((distance(0, 0x171723) - (23. + 23. + 35.) / 765.).abs() < 1e-12);
        let all = palette(false).to_vec();
        let picked = suggest(false, &all);
        assert!(palette(false).contains(&picked));
    }
}
