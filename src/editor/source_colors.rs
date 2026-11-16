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
    paste_waiting: bool,
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
            paste_waiting: false,
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
/// Handle a requested clipboard read before definition clipboard shortcuts see its paste event.
pub(in crate::app) fn handle_color_clipboard(ctx: &egui::Context) -> bool {
    let Some(id) = ctx.data_mut(|d| d.get_temp::<egui::Id>(active_color_id())) else {
        return false;
    };
    let Some(mut state) = ctx.data_mut(|d| d.get_temp::<ColorDraft>(id)) else {
        return false;
    };
    if !state.paste_waiting {
        return false;
    }
    let raw = ctx.input(|i| {
        i.events.iter().find_map(|event| {
            if let egui::Event::Paste(raw) = event {
                Some(raw.clone())
            } else {
                None
            }
        })
    });
    if let Some(raw) = raw {
        if let Some(rgb) = source_hex(&raw) {
            state.assign(rgb);
        }
        state.paste_waiting = false;
        ctx.data_mut(|d| d.insert_temp(id, state));
        return true;
    }
    false
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
    let mut mesh = egui::Mesh::default();
    for (pos, col) in [
        (plane.left_top(), Color32::WHITE),
        (
            plane.right_top(),
            Color32::from_rgb(full[0], full[1], full[2]),
        ),
        (plane.left_bottom(), Color32::BLACK),
        (plane.right_bottom(), Color32::BLACK),
    ] {
        mesh.colored_vertex(pos, col);
    }
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(1, 2, 3);
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
    ui.painter().circle_filled(selected, 6., Color32::WHITE);
    ui.painter()
        .circle_stroke(selected, 7., Stroke::new(1_f32, Color32::BLACK));
    let y = hue.top() + (1. - state.hsv[0]) as f32 * 148.;
    ui.painter().rect_filled(
        Rect::from_min_size(egui::pos2(hue.right() - 7., y), egui::vec2(10., 2.)),
        0.,
        Color32::WHITE,
    );
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
    let response = source_controls::add_sized_button(
        ui,
        [38., 24.],
        egui::Button::new(" ")
            .fill(Color32::from_rgb(color[0], color[1], color[2]))
            .stroke(Stroke::new(1_f32, Color32::GRAY)),
    );
    color_picker(ui, color, tag, response, false, false).0
}
fn color_picker(
    ui: &mut egui::Ui,
    color: &mut [u8; 3],
    tag: Option<&str>,
    mut response: egui::Response,
    force_open: bool,
    allow_null: bool,
) -> (egui::Response, Option<bool>) {
    let mut validated = None;
    let id = response.id.with("source-color-draft");
    let mut state = ui.ctx().data_mut(|d| d.get_temp::<ColorDraft>(id));
    let just_opened = (response.clicked() || force_open) && state.is_none();
    if just_opened {
        let mut draft = ColorDraft::new(*color);
        draft.tag = tag.map(str::to_owned);
        state = Some(draft);
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
                            let mut text = draft.hex.clone();
                            let field = ui.add_sized(
                                [if allow_null { 176. } else { 224. }, 44.],
                                egui::TextEdit::singleline(&mut text)
                                    .hint_text("rrggbb")
                                    .margin(egui::Margin {
                                        left: 17,
                                        right: 4,
                                        top: 6,
                                        bottom: 6,
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
                                ),
                            )
                            .on_hover_text("Copy color")
                            .clicked()
                            {
                                ui.ctx().copy_text(format!("#{}", hex(draft.rgb)));
                                draft.copied = true;
                            }
                            if source_controls::add_sized_button(
                                ui,
                                [44., 44.],
                                egui::Button::image(
                                    egui::Image::new(egui::include_image!(
                                        "../../assets/icons/paste.svg"
                                    ))
                                    .fit_to_exact_size(Vec2::splat(24.)),
                                ),
                            )
                            .on_hover_text("Paste color")
                            .clicked()
                            {
                                draft.paste_waiting = true;
                                ui.ctx()
                                    .send_viewport_cmd(egui::ViewportCommand::RequestPaste);
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
                                .on_hover_text("Remove color")
                                .clicked()
                            {
                                draft.nullified = true;
                                ui.close();
                            }
                        });
                        spectrum(ui, &mut draft);
                        let blind = ui
                            .ctx()
                            .data_mut(|d| {
                                d.get_temp::<bool>(egui::Id::new("source-colorblind-palette"))
                            })
                            .unwrap_or(false);
                        let mut choices = palette(blind).to_vec();
                        if let Some(tag) = &draft.tag {
                            let key = egui::Id::new("source-show-project-colors");
                            let mut expanded = ui
                                .ctx()
                                .data_mut(|d| d.get_temp::<bool>(key))
                                .unwrap_or(true);
                            let previous = expanded;
                            if ui
                                .selectable_label(
                                    expanded,
                                    if draft.show_all {
                                        "All colors used in this project"
                                    } else {
                                        "Colors used for this element"
                                    },
                                )
                                .clicked()
                            {
                                expanded = !expanded;
                            }
                            if expanded != previous {
                                ui.ctx().data_mut(|d| {
                                    d.insert_temp(key, expanded);
                                    d.insert_temp(key.with("changed"), expanded);
                                });
                            }
                            let used = ui
                                .ctx()
                                .data_mut(|d| d.get_temp::<UsedColors>(key.with("used")))
                                .unwrap_or_default();
                            choices = if !expanded {
                                vec![]
                            } else if draft.show_all {
                                let mut all =
                                    used.0.values().flatten().copied().collect::<Vec<_>>();
                                all.sort_by(|a, b| {
                                    rgb_hsv(rgb(*a))[0].total_cmp(&rgb_hsv(rgb(*b))[0])
                                });
                                all
                            } else {
                                used.0.get(tag).cloned().unwrap_or_default()
                            };
                            if expanded && choices.is_empty() {
                                ui.weak("Empty");
                            }
                            if expanded
                                && !draft.show_all
                                && ui.link("Show all colors used in this project").clicked()
                            {
                                draft.show_all = true;
                            }
                        }
                        egui::Grid::new(id.with("palette"))
                            .min_col_width(0.)
                            .spacing(Vec2::splat(0.))
                            .show(ui, |ui| {
                                for (index, choice) in choices.iter().enumerate() {
                                    let value = rgb(*choice);
                                    let (rect, r) = ui.allocate_exact_size(
                                        egui::vec2(320. / 12., 24.),
                                        Sense::click(),
                                    );
                                    ui.painter().rect_filled(
                                        rect,
                                        0.,
                                        Color32::from_rgb(value[0], value[1], value[2]),
                                    );
                                    ui.painter().rect_stroke(
                                        rect,
                                        0.,
                                        Stroke::new(
                                            if r.hovered() { 2_f32 } else { 1_f32 },
                                            if r.hovered() {
                                                Color32::WHITE
                                            } else {
                                                Color32::BLACK
                                            },
                                        ),
                                        StrokeKind::Inside,
                                    );
                                    if r.on_hover_text(format!("#{choice:06X}")).clicked() {
                                        draft.assign(value);
                                    }
                                    if index % 12 == 11 {
                                        ui.end_row();
                                    }
                                }
                            });
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
    let mut rgb = color.unwrap_or([0, 0, 0]);
    let response = source_controls::add_sized_button(
        ui,
        [38., 24.],
        egui::Button::new(" ")
            .fill(Color32::from_rgb(rgb[0], rgb[1], rgb[2]))
            .stroke(Stroke::new(1_f32, Color32::GRAY)),
    );
    let (mut response, validation) = color_picker(ui, &mut rgb, None, response, false, true);
    if let Some(null) = validation {
        let next = if null { None } else { Some(rgb) };
        if *color != next {
            *color = next;
            response.mark_changed();
        }
    }
    response
}
/// Home directory context menus open this same picker immediately, without an extra swatch click.
pub(in crate::app) fn directory_color_dialog(
    ctx: &egui::Context,
    id: egui::Id,
    color: &mut [u8; 3],
    first_frame: bool,
) -> bool {
    egui::Area::new(id)
        .order(egui::Order::Foreground)
        .fixed_pos(ctx.content_rect().center() - egui::vec2(160., 160.))
        .show(ctx, |ui| {
            let (_, response) = ui.allocate_exact_size(Vec2::ZERO, Sense::hover());
            color_picker(ui, color, None, response, first_frame, false);
        });
    !picker_is_open(ctx)
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
        for _ in 0..4 {
            run(vec![], &mut color);
        }
        let rect = picker_rect(&ctx).expect("open picker");
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
        let mut state = ColorDraft::new([0, 0, 0]);
        state.paste_waiting = true;
        ctx.data_mut(|d| {
            d.insert_temp(active_color_id(), id);
            d.insert_temp(id, state);
        });
        let _ = ctx.run(
            egui::RawInput {
                events: vec![egui::Event::Paste("#0af".into())],
                ..Default::default()
            },
            |ctx| assert!(handle_color_clipboard(ctx)),
        );
        assert_eq!(
            ctx.data_mut(|d| d.get_temp::<ColorDraft>(id)).unwrap().rgb,
            [0, 170, 255]
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
