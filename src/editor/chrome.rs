use super::*;
use egui::emath::GuiRounding as _;

pub(super) const SIDEBAR_WIDTH: f32 = 300.;
pub(super) const ZEN_EDGE_WIDTH: f32 = 17.;
const SIDEBAR_GUIDE_FILL: Color32 = Color32::from_rgb(39, 63, 47);

#[derive(Default)]
pub(super) struct ZenSidebar {
    pub revealed: bool,
    hide_after: Option<f64>,
}

fn show_sidebar(ctx: &egui::Context, overlay: bool, contents: impl FnOnce(&mut egui::Ui)) {
    let panel = egui::SidePanel::left("main-panel")
        .exact_width(SIDEBAR_WIDTH)
        .resizable(false)
        .show_separator_line(false)
        .frame(egui::Frame::new().fill(theme::DARK));
    let contents = |ui: &mut egui::Ui| {
        let rect = ui.max_rect();
        contents(ui);
        ui.painter().rect_filled(
            Rect::from_min_max(egui::pos2(rect.right() - 2., rect.top()), rect.max),
            0.,
            Color32::BLACK,
        );
    };
    if overlay {
        let screen = ctx.content_rect();
        let size = egui::vec2(SIDEBAR_WIDTH.min(screen.width()), screen.height());
        egui::Area::new(egui::Id::new("zen-sidebar"))
            .order(egui::Order::Middle)
            .fixed_pos(screen.min)
            .default_size(size)
            .constrain(false)
            .fade_in(false)
            .show(ctx, |ui| {
                ui.set_min_size(size);
                ui.set_max_size(size);
                ui.set_clip_rect(Rect::from_min_size(screen.min, size));
                panel.show_inside(ui, contents);
            });
    } else {
        panel.show(ctx, contents);
    }
}

fn short_world_name(name: &str) -> String {
    const MAX_LEN: usize = 6;
    if name.chars().count() <= MAX_LEN {
        return name.to_owned();
    }
    let mut short = String::with_capacity(MAX_LEN);
    let mut picks = 2;
    let mut uppercase_next = false;
    let mut tail = name;
    let mut count = 0;
    for (offset, ch) in name.char_indices() {
        if ch == '_' {
            picks = 2;
            uppercase_next = true;
            continue;
        }
        if ch.is_ascii_uppercase() || ch.is_ascii_digit() {
            picks = 2;
        }
        if picks == 0 {
            continue;
        }
        short.push(if uppercase_next {
            ch.to_ascii_uppercase()
        } else {
            ch
        });
        picks -= 1;
        uppercase_next = false;
        tail = &name[offset + ch.len_utf8()..];
        count += 1;
        if count == MAX_LEN {
            return short;
        }
    }
    short.extend(tail.chars().take(MAX_LEN - count));
    short
}

fn shortcut_label(key: &str) -> &str {
    match key {
        "SHIFT" => "⇧",
        "CTRL" if cfg!(target_os = "macos") => "⌘",
        _ => key,
    }
}

fn shortcut_width(ui: &egui::Ui, key: &str) -> f32 {
    ui.painter()
        .layout_no_wrap(
            shortcut_label(key).into(),
            FontId::new(13.2, egui::FontFamily::Name("heading".into())),
            Color32::WHITE,
        )
        .size()
        .x
        .max(16.)
        + 12.
}

fn paint_shortcut(ui: &egui::Ui, rect: Rect, key: &str, shadow: bool) {
    paint_keycap(ui, rect, shortcut_label(key), Color32::WHITE, shadow);
}

pub(super) fn paint_keycap(ui: &egui::Ui, rect: Rect, label: &str, tint: Color32, shadow: bool) {
    let cap = rect.shrink(1.);
    if let Ok(egui::load::TexturePoll::Ready { texture }) =
        egui::Image::new(egui::include_image!("../../assets/img/keycap.svg"))
            .texture_options(egui::TextureOptions::LINEAR)
            .fit_to_exact_size(Vec2::splat(32.))
            .load_for_size(ui.ctx(), Vec2::splat(32.))
    {
        // Preserve the CSS radius, bottom border and outside shadow at any key width.
        let bounds = Rect::from_min_max(cap.min - egui::vec2(2., 2.), cap.max + egui::vec2(2., 6.));
        let xs = [
            bounds.left(),
            bounds.left() + 4.,
            bounds.right() - 4.,
            bounds.right(),
        ];
        let ys = [
            bounds.top(),
            bounds.top() + 4.,
            bounds.bottom() - 12.,
            bounds.bottom(),
        ];
        let us = [0., 4. / 32., 28. / 32., 1.];
        let vs = [0., 4. / 32., 20. / 32., 1.];
        let mut mesh = egui::Mesh::with_texture(texture.id);
        for y in 0..3 {
            for x in 0..3 {
                mesh.add_rect_with_uv(
                    Rect::from_min_max(egui::pos2(xs[x], ys[y]), egui::pos2(xs[x + 1], ys[y + 1])),
                    Rect::from_min_max(egui::pos2(us[x], vs[y]), egui::pos2(us[x + 1], vs[y + 1])),
                    tint,
                );
            }
        }
        if shadow {
            ui.painter().add(mesh);
        } else {
            ui.painter().with_clip_rect(cap).add(mesh);
        }
    }
    let font = FontId::new(13.2, egui::FontFamily::Name("heading".into()));
    let pos = cap.center() - egui::vec2(0., 2.);
    ui.painter().text(
        pos + egui::vec2(0., 1.),
        Align2::CENTER_CENTER,
        label,
        font.clone(),
        Color32::from_black_alpha(128),
    );
    ui.painter()
        .text(pos, Align2::CENTER_CENTER, label, font, Color32::WHITE);
}

fn empty_layer_list(ui: &mut egui::Ui) {
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width() - 2., 100.), Sense::hover());
    egui::Image::new(egui::include_image!("../../assets/img/stripes.png"))
        .texture_options(egui::TextureOptions::NEAREST_REPEAT)
        .uv(Rect::from_min_size(egui::Pos2::ZERO, rect.size() / 8.))
        .paint_at(ui, rect);
    ui.painter().rect_stroke(
        rect,
        0.,
        Stroke::new(1_f32, theme::MEDIUM),
        StrokeKind::Inside,
    );
    let caption = egui::WidgetText::from(
        RichText::new(locale::tr(ui, "No layer"))
            .font(FontId::new(
                19.067,
                egui::FontFamily::Name("panel-title".into()),
            ))
            .italics()
            .color(theme::LIGHT),
    )
    .into_galley(ui, None, rect.width(), egui::TextStyle::Body);
    let center = egui::pos2(rect.center().x, rect.top() + 44.5);
    ui.painter()
        .galley(center - caption.size() * 0.5, caption, theme::LIGHT);
}

fn sidebar_guide(ui: &mut egui::Ui, kind: &str, empty: bool) {
    let hints: &[(&[&str], &str)] = if empty {
        &[(&[], "Need at least 1 layer")]
    } else {
        match kind {
            "Entities" => &[(&["ALT"], "Pick"), (&["CTRL", "ALT"], "Copy")],
            "IntGrid" | "Tiles" => &[(&["SHIFT"], "Rectangle"), (&["ALT"], "Pick")],
            _ => return,
        }
    };
    let key_width = |key: &str| match key {
        "ALT" => 26.625,
        "CTRL" if !cfg!(target_os = "macos") => 34.78125,
        _ => 26.,
    };
    let color = Color32::from_rgb(121, 221, 138);
    let font = FontId::new(13.2, egui::FontFamily::Name("panel-title".into()));
    let guide = egui::Frame::new()
        .fill(SIDEBAR_GUIDE_FILL)
        .inner_margin(egui::Margin {
            left: 4,
            right: 4,
            top: 5,
            bottom: 6,
        })
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.spacing_mut().interact_size.y = 0.;
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = egui::vec2(3., 1.);
                for (keys, label) in hints {
                    let caption = egui::WidgetText::from(
                        RichText::new(locale::tr(ui, label))
                            .font(font.clone())
                            .italics()
                            .color(color),
                    )
                    .into_galley(
                        ui,
                        Some(egui::TextWrapMode::Extend),
                        f32::INFINITY,
                        egui::TextStyle::Body,
                    );
                    let mouse_size = if keys.is_empty() { 0. } else { 17.15625 };
                    let keys_width = keys.iter().map(|key| key_width(key) + 1.).sum::<f32>();
                    let width = 1. + keys_width + mouse_size + caption.size().x + 8.;
                    let height = if keys.is_empty() { 16.15625 } else { 20.0625 };
                    let (rect, _) =
                        ui.allocate_exact_size(egui::vec2(width, height), Sense::hover());
                    ui.painter()
                        .rect_filled(rect, 3., Color32::from_rgb(60, 97, 72));
                    let mut x = rect.left() + 1.;
                    for key in *keys {
                        let w = key_width(key);
                        let cap =
                            Rect::from_min_size(egui::pos2(x, rect.top()), egui::vec2(w, 20.));
                        paint_shortcut(ui, cap.expand(1.), key, false);
                        x += w + 1.;
                    }
                    if mouse_size > 0. {
                        egui::Image::new(egui::include_image!("../../assets/icons/mouseLeft.png"))
                            .texture_options(egui::TextureOptions::NEAREST)
                            .paint_at(
                                ui,
                                Rect::from_min_size(
                                    egui::pos2(x, rect.top()),
                                    Vec2::splat(mouse_size),
                                ),
                            );
                    }
                    ui.painter()
                        .galley(egui::pos2(x + mouse_size, rect.top()), caption, color);
                }
            });
        })
        .response
        .rect;
    let edge = Stroke::new(1_f32, Color32::from_rgb(94, 171, 107));
    ui.painter().hline(guide.x_range(), guide.top() + 0.5, edge);
    ui.painter()
        .hline(guide.x_range(), guide.bottom() - 0.5, edge);
    ui.add_space(16.);
}

pub(super) fn source_tooltip_popup<'a>(
    response: &'a egui::Response,
    placement: Option<egui::emath::RectAlign>,
    padding: egui::Margin,
) -> egui::Tooltip<'a> {
    use egui::emath::RectAlign;

    let screen = response.ctx.content_rect();
    let target = response.rect;
    let above = target.height() <= 48. && target.top() >= screen.top() + 40.
        || target.bottom() + 8. >= screen.bottom() - 150.;
    let align_right = target.left() >= screen.left() + screen.width() * 0.7;
    let align = placement.unwrap_or(match (above, align_right) {
        (true, true) => RectAlign::TOP_END,
        (true, false) => RectAlign::TOP_START,
        (false, true) => RectAlign::BOTTOM_END,
        (false, false) => RectAlign::BOTTOM_START,
    });
    let mut tooltip = egui::Tooltip::for_widget(response)
        .width(0.)
        .gap(if placement.is_some() { 10. } else { 8. });
    tooltip.popup = tooltip.popup.align(align).frame(
        egui::Frame::new()
            .fill(Color32::from_rgb(60, 97, 72))
            .corner_radius(4)
            .inner_margin(padding)
            .shadow(egui::epaint::Shadow {
                offset: [0, 8],
                blur: 16,
                spread: 0,
                color: Color32::from_black_alpha(77),
            }),
    );
    tooltip
}

pub(super) fn source_tooltip(response: &egui::Response, text: &str, keys: &[&str], right: bool) {
    if !response.hovered() || !response.enabled() {
        return;
    }
    let tooltip = source_tooltip_popup(
        response,
        right.then_some(egui::emath::RectAlign::RIGHT_START),
        egui::Margin::symmetric(18, 12),
    );
    tooltip.show(|ui| {
        let text = locale::tr(ui, text);
        let (heading, body) = text
            .strip_prefix("**")
            .and_then(|text| text.split_once("**"))
            .map_or((None, text), |(heading, body)| (Some(heading), body.trim()));
        let body = ui.painter().layout(
            body.replace("\\n", "\n\n"),
            FontId::proportional(14.666667),
            Color32::from_rgb(121, 221, 138),
            250.,
        );
        let heading = heading.map(|heading| {
            ui.painter().layout_no_wrap(
                heading.into(),
                FontId::new(17.6, egui::FontFamily::Name("heading".into())),
                Color32::WHITE,
            )
        });
        let heading_height = heading.as_ref().map_or(0., |heading| heading.size().y + 4.);
        let text_size = egui::vec2(
            heading
                .as_ref()
                .map_or(body.size().x, |heading| heading.size().x.max(body.size().x)),
            body.size().y + heading_height,
        );
        let key_widths = keys
            .iter()
            .map(|key| shortcut_width(ui, key))
            .collect::<Vec<_>>();
        let keys_width = key_widths.iter().sum::<f32>();
        let (rect, _) = ui.allocate_exact_size(
            egui::vec2(
                text_size.x
                    + if keys.is_empty() {
                        0.
                    } else {
                        20. + keys_width
                    },
                text_size.y.max(23.5),
            ),
            Sense::hover(),
        );
        let text_pos = rect.left_center() - egui::vec2(0., text_size.y * 0.5);
        if let Some(heading) = heading {
            ui.painter().galley_with_override_text_color(
                text_pos + egui::vec2(0., 1.),
                heading.clone(),
                Color32::from_black_alpha(128),
            );
            ui.painter().galley(text_pos, heading, Color32::WHITE);
        }
        let body_pos = text_pos + egui::vec2(0., heading_height);
        ui.painter().galley_with_override_text_color(
            body_pos + egui::vec2(0., 1.),
            body.clone(),
            Color32::from_black_alpha(128),
        );
        ui.painter()
            .galley(body_pos, body, Color32::from_rgb(121, 221, 138));
        let mut x = rect.right() - keys_width;
        for (key, width) in keys.iter().zip(key_widths) {
            paint_shortcut(
                ui,
                Rect::from_min_size(
                    egui::pos2(x, rect.center().y - 11.75),
                    egui::vec2(width, 23.5),
                ),
                key,
                true,
            );
            x += width;
        }
    });
}

fn canvas_option_heading(ui: &mut egui::Ui, title: &str, color: Color32) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(34., 18.), Sense::hover());
    ui.painter().text(
        rect.center(),
        Align2::CENTER_CENTER,
        locale::tr(ui, title),
        FontId::new(12., egui::FontFamily::Name("panel-title".into())),
        color,
    );
    response
}

fn canvas_option(
    ui: &mut egui::Ui,
    source: egui::ImageSource<'static>,
    active: bool,
    supported: bool,
    visual: bool,
    tip: &str,
    keys: &[&str],
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(34.), Sense::click());
    let label = tip
        .trim_start_matches("**")
        .split("**")
        .next()
        .unwrap_or(tip);
    response.widget_info(|| {
        egui::WidgetInfo::selected(
            egui::WidgetType::Checkbox,
            true,
            active,
            locale::tr(ui, label),
        )
    });
    let (fill, border, glow, faded_icon) = match (visual, active) {
        (true, true) => (
            [101, 148, 255],
            [141, 211, 255],
            [101, 148, 255],
            [255, 255, 255],
        ),
        (true, false) => (
            [30, 54, 111],
            [101, 148, 255],
            [101, 148, 255],
            [76, 112, 201],
        ),
        (false, true) => (
            [60, 97, 72],
            [207, 221, 64],
            [121, 221, 138],
            [255, 255, 255],
        ),
        (false, false) => (
            [39, 63, 47],
            [94, 171, 107],
            [121, 221, 138],
            [199, 205, 98],
        ),
    };
    let rgb = |[r, g, b]: [u8; 3]| Color32::from_rgb(r, g, b);
    let painter = ui.painter();
    let opacity = if active { 1. } else { 0.66 };
    if active {
        painter.add(
            egui::epaint::Shadow {
                offset: [0, 0],
                blur: 4,
                spread: 0,
                color: rgb(glow),
            }
            .as_shape(rect, 3.),
        );
    }
    painter.rect_filled(rect, 3., rgb(fill).gamma_multiply(opacity));
    if !supported {
        egui::Image::new(egui::include_image!("../../assets/img/stripes.png"))
            .texture_options(egui::TextureOptions::NEAREST_REPEAT)
            .uv(Rect::from_min_size(egui::Pos2::ZERO, rect.size() / 8.))
            .tint(Color32::WHITE.gamma_multiply(opacity))
            .corner_radius(3)
            .paint_at(ui, rect);
    }
    if active {
        for inset in 1..=10 {
            painter.rect_stroke(
                rect.shrink(inset as f32),
                (3 - inset).max(0) as f32,
                Stroke::new(1_f32, rgb(border).gamma_multiply((12 - inset) as f32 / 16.)),
                egui::StrokeKind::Inside,
            );
        }
    }
    let border = if response.hovered() && supported {
        Color32::WHITE
    } else {
        rgb(border)
    };
    painter.rect_stroke(
        rect,
        3.,
        Stroke::new(1_f32, border.gamma_multiply(opacity)),
        egui::StrokeKind::Inside,
    );
    let icon_opacity = if !supported {
        0.2
    } else if active || visual {
        1.
    } else {
        0.6
    };
    egui::Image::new(source)
        .tint(rgb(faded_icon).gamma_multiply(icon_opacity))
        .paint_at(ui, Rect::from_center_size(rect.center(), Vec2::splat(24.)));
    let response = response.on_hover_cursor(if supported {
        egui::CursorIcon::PointingHand
    } else {
        egui::CursorIcon::NotAllowed
    });
    // The source rebuilds the option tips after a click. Hovering again requires re-entering the row.
    let tooltip_lock = response.id.with("tooltip-dismissed");
    let contains_pointer = response.contains_pointer();
    let clicked = response.clicked();
    let dismissed = ui.data_mut(|data| {
        if !contains_pointer {
            data.remove::<bool>(tooltip_lock);
        } else if clicked {
            data.insert_temp(tooltip_lock, true);
        }
        data.get_temp::<bool>(tooltip_lock).unwrap_or(false)
    });
    if !dismissed {
        source_tooltip(&response, tip, keys, true);
    }
    response
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ToolbarButtonState {
    Normal,
    Active,
    Faded,
}

const CLOSE_TOOLBAR_COLOR: Color32 = Color32::from_rgb(49, 55, 66);

fn paint_toolbar_icon(ui: &egui::Ui, source: egui::ImageSource<'static>, center: egui::Pos2) {
    let icon_size = Vec2::splat(20.);
    let (uri, bytes) = match &source {
        egui::ImageSource::Bytes { uri, bytes } if uri.ends_with(".png") => (uri, bytes),
        _ => {
            egui::Image::new(source).paint_at(ui, Rect::from_center_size(center, icon_size));
            return;
        }
    };

    let ctx = ui.ctx();
    let scale = ui.pixels_per_point();
    let id = egui::Id::new(("toolbar-bitmap", uri.as_ref()));
    let cached = ctx
        .data(|data| data.get_temp::<(f32, Vec2, egui::TextureHandle)>(id))
        .filter(|(cached_scale, _, _)| *cached_scale == scale);
    let (_, size, texture) = if let Some(cached) = cached {
        cached
    } else {
        ctx.include_bytes(uri.clone(), bytes.clone());
        let Ok(egui::load::ImagePoll::Ready { image }) =
            ctx.try_load_image(uri, Default::default())
        else {
            return;
        };
        let [width, height] = image.size;
        let size =
            egui::vec2(width as f32, height as f32) * (icon_size.x / width.max(height) as f32);
        let target = (size * scale).round().max(Vec2::splat(1.));
        let rgba = image
            .pixels
            .iter()
            .flat_map(|pixel| pixel.to_array())
            .collect();
        let Some(original) = image::RgbaImage::from_raw(width as u32, height as u32, rgba) else {
            return;
        };
        // Resample premultiplied pixels so transparent texels do not bleed into the icon.
        let mut resized = image::imageops::resize(
            &original,
            target.x as u32,
            target.y as u32,
            image::imageops::FilterType::CatmullRom,
        );
        for pixel in resized.pixels_mut() {
            let alpha = pixel[3];
            for channel in &mut pixel.0[..3] {
                *channel = (*channel).min(alpha);
            }
        }
        let resized = egui::ColorImage::from_rgba_premultiplied(
            [target.x as usize, target.y as usize],
            resized.as_raw(),
        );
        let texture = ctx.load_texture(uri.as_ref(), resized, egui::TextureOptions::LINEAR);
        let cached = (scale, size, texture);
        ctx.data_mut(|data| data.insert_temp(id, cached.clone()));
        cached
    };
    // Chromium snaps the origin and dimensions separately when drawing the bitmap.
    let origin = (center - size * 0.5).round_to_pixels(scale);
    let rect = Rect::from_min_size(origin, texture.size_vec2() / scale);
    egui::Image::from_texture(egui::load::SizedTexture::from_handle(&texture)).paint_at(ui, rect);
}

fn toolbar_button(
    ui: &mut egui::Ui,
    rect: Rect,
    image: egui::ImageSource<'static>,
    (normal, hover): (Color32, Color32),
    state: ToolbarButtonState,
    tip: &str,
    keys: &[&str],
) -> egui::Response {
    let response = ui
        .interact(rect, ui.id().with(tip), Sense::click())
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    response.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::WidgetType::Button,
            response.enabled(),
            locale::tr(ui, tip),
        )
    });
    let painter = ui.painter();
    let fill = if state == ToolbarButtonState::Active {
        theme::LIGHT
    } else if response.hovered() {
        hover
    } else {
        normal
    };
    painter.rect_filled(rect, 0., fill);
    if state == ToolbarButtonState::Normal {
        if normal != CLOSE_TOOLBAR_COLOR {
            painter.rect_filled(
                Rect::from_min_max(egui::pos2(rect.left(), rect.bottom() - 2.), rect.max),
                0.,
                Color32::from_black_alpha(77),
            );
        }
        let edge = if normal == CLOSE_TOOLBAR_COLOR {
            theme::LIGHT.gamma_multiply(0.26)
        } else {
            Color32::from_white_alpha(15)
        };
        painter.rect_stroke(rect, 0., Stroke::new(1_f32, edge), StrokeKind::Inside);
        painter.rect_stroke(
            rect.shrink(1.),
            0.,
            Stroke::new(1_f32, edge.gamma_multiply(0.2)),
            StrokeKind::Inside,
        );
    } else if state == ToolbarButtonState::Faded {
        painter.rect_filled(
            Rect::from_min_max(egui::pos2(rect.left(), rect.bottom() - 6.), rect.max),
            0.,
            Color32::from_black_alpha(102),
        );
    }
    paint_toolbar_icon(ui, image, rect.center());
    if state == ToolbarButtonState::Faded {
        // CSS fades the finished button, including its icon and inset shadow.
        let [r, g, b, _] = theme::DARK.to_array();
        painter.rect_filled(
            rect,
            0.,
            Color32::from_rgba_premultiplied(r / 2, g / 2, b / 2, 127),
        );
    }
    source_tooltip(&response, tip, keys, false);
    response
}

impl EditorApp {
    pub(super) fn layer_filter_popup_id() -> egui::Id {
        egui::Id::new("layer-filter-popup")
    }

    fn layer_filter_dropdown(ui: &mut egui::Ui, tags: &[String], selected: &mut i64) {
        let all = locale::tr(ui, "- Show all layers -");
        let name = tags.get(*selected as usize).map_or(all, String::as_str);
        let width = ui.available_width() - 2.;
        let (rect, response) = ui.allocate_exact_size(egui::vec2(width, 28.), Sense::click());
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::WidgetType::ComboBox, ui.is_enabled(), name)
        });
        let id = Self::layer_filter_popup_id();
        let open = egui::Popup::is_id_open(ui.ctx(), id);
        let stroke = if open || response.hovered() {
            Color32::WHITE
        } else {
            Color32::from_rgb(115, 127, 157)
        };
        let painter = ui.painter();
        painter.rect(
            rect,
            3.,
            theme::DARK,
            Stroke::new(1_f32, stroke),
            StrokeKind::Inside,
        );
        let text_rect = rect.shrink2(egui::vec2(7., 0.));
        painter
            .with_clip_rect(text_rect.with_max_x(rect.right() - 20.))
            .text(
                text_rect.left_center(),
                Align2::LEFT_CENTER,
                name,
                FontId::proportional(12.),
                theme::LIGHT,
            );
        let arrow = rect.right_center() - egui::vec2(9., 0.);
        painter.add(egui::Shape::line(
            vec![
                arrow + egui::vec2(-4., -2.),
                arrow + egui::vec2(0., 2.),
                arrow + egui::vec2(4., -2.),
            ],
            Stroke::new(1.5_f32, theme::LIGHT),
        ));

        let options = std::iter::once(all)
            .chain(tags.iter().map(String::as_str))
            .map(|label| (label, Color32::WHITE))
            .collect::<Vec<_>>();
        if let Some(picked) =
            panels::source_controls::select_popup(&response, id, (*selected + 1) as usize, &options)
        {
            *selected = picked as i64 - 1;
        }
    }

    fn layer_filter_tags(&self) -> Vec<&str> {
        let mut tags = self
            .project
            .as_ref()
            .and_then(|p| p.json["defs"]["layers"].as_array())
            .into_iter()
            .flatten()
            .flat_map(|definition| definition["uiFilterTags"].as_array().into_iter().flatten())
            .filter_map(Value::as_str)
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        tags.sort_by_key(|tag| tag.to_lowercase());
        tags
    }

    pub(super) fn visible_layer_indices(&self, include_active: bool) -> Vec<usize> {
        let Some(project) = &self.project else {
            return vec![];
        };
        let Some(level) = self.selected.and_then(|k| project.level(k)) else {
            return vec![];
        };
        let selected =
            self.settings
                .ui_state_int("LayerUIFilter", project.json["iid"].as_str(), -1);
        let tags = self.layer_filter_tags();
        let filter = tags.get(selected as usize).copied();
        level["layerInstances"]
            .as_array()
            .into_iter()
            .flatten()
            .enumerate()
            .filter(|(i, l)| {
                let d = l["layerDefUid"]
                    .as_i64()
                    .and_then(|uid| project.layer_def(uid));
                (include_active && *i == self.layer)
                    || d.is_some_and(|d| {
                        d["hideInList"] != true
                            && filter.is_none_or(|filter| {
                                d["uiFilterTags"]
                                    .as_array()
                                    .is_some_and(|a| a.iter().any(|t| t == filter))
                            })
                    })
            })
            .map(|(i, _)| i)
            .collect()
    }

    fn layer_filter_controls(&mut self, ui: &mut egui::Ui, project_iid: Option<&str>) {
        let tags = self
            .layer_filter_tags()
            .into_iter()
            .map(str::to_owned)
            .collect::<Vec<_>>();
        let stored = self.settings.ui_state_int("LayerUIFilter", project_iid, -1);
        if tags.is_empty() {
            if stored >= 0 {
                self.settings.delete_ui_state("LayerUIFilter", project_iid);
            }
            return;
        }
        let old = tags.get(stored as usize).map_or(-1, |_| stored);
        let mut selected = old;
        if tags.len() > 3 {
            Self::layer_filter_dropdown(ui, &tags, &mut selected);
        } else {
            egui::Frame::new()
                .fill(theme::MEDIUM)
                .inner_margin(1)
                .show(ui, |ui| {
                    ui.set_min_width(ui.available_width());
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing = egui::vec2(1., 1.);
                        let font = FontId::proportional(10.667);
                        let all = locale::tr(ui, "All");
                        for (index, tag) in std::iter::once(all)
                            .chain(tags.iter().map(String::as_str))
                            .enumerate()
                        {
                            let index = index as i64 - 1;
                            let active = index == selected;
                            let foreground = if active {
                                Color32::BLACK
                            } else {
                                Color32::WHITE
                            };
                            let galley = ui.painter().layout_no_wrap(
                                tag.to_owned(),
                                font.clone(),
                                foreground,
                            );
                            let width = galley.size().x.max(2. * font.size) + 14.;
                            let (rect, response) =
                                ui.allocate_exact_size(egui::vec2(width, 23.), Sense::click());
                            response.widget_info(|| {
                                egui::WidgetInfo::selected(
                                    egui::WidgetType::SelectableLabel,
                                    true,
                                    active,
                                    tag,
                                )
                            });
                            let border = if active || response.hovered() {
                                theme::ACCENT
                            } else {
                                theme::LIGHT
                            };
                            ui.painter().rect(
                                rect,
                                3.,
                                if active {
                                    theme::ACCENT
                                } else {
                                    Color32::TRANSPARENT
                                },
                                Stroke::new(1_f32, border),
                                StrokeKind::Inside,
                            );
                            ui.painter().galley(
                                rect.center() - galley.size() * 0.5,
                                galley,
                                foreground,
                            );
                            if response.clicked() {
                                selected = index;
                            }
                        }
                    });
                });
        }
        ui.add_space(1.);
        if selected != old {
            if selected < 0 {
                self.settings.delete_ui_state("LayerUIFilter", project_iid);
            } else {
                self.settings
                    .set_ui_state_int("LayerUIFilter", project_iid, selected);
            }
            let eligible = self.visible_layer_indices(false);
            if !eligible.contains(&self.layer)
                && let Some(&first) = eligible.first()
            {
                self.switch_layer(first);
            }
            ui.ctx().request_repaint();
        }
    }

    fn update_zen_sidebar(&mut self, ctx: &egui::Context) {
        if !self.zen || self.project.is_none() {
            self.zen_sidebar = ZenSidebar::default();
            return;
        }
        let screen = ctx.content_rect();
        let edge = Rect::from_min_size(screen.min, egui::vec2(ZEN_EDGE_WIDTH, screen.height()));
        let edge_id = egui::Id::new("zen-sidebar-edge");
        let (pointer, moved, now) = ctx.input(|input| {
            (
                input.pointer.hover_pos(),
                input.pointer.delta() != Vec2::ZERO,
                input.time,
            )
        });
        let layer = pointer.and_then(|pointer| ctx.layer_id_at(pointer));
        let canvas_at_pointer = layer.is_none_or(|layer| layer.order == egui::Order::Background);
        if self.zen_sidebar.revealed {
            let canvas_pointer = pointer.filter(|pointer| {
                canvas_at_pointer && (self.world || pointer.x >= screen.left() + SIDEBAR_WIDTH)
            });
            if let Some(pointer) = canvas_pointer {
                let hide_after = self.zen_sidebar.hide_after.get_or_insert(now + 1.);
                if moved && pointer.x > screen.left() + SIDEBAR_WIDTH + 200. {
                    *hide_after = hide_after.min(now + 0.2);
                }
                if now >= *hide_after {
                    self.zen_sidebar = ZenSidebar::default();
                } else {
                    ctx.request_repaint_after(std::time::Duration::from_secs_f64(
                        *hide_after - now,
                    ));
                }
            } else {
                self.zen_sidebar.hide_after = None;
            }
        } else if pointer.is_some_and(|pointer| edge.contains(pointer))
            && (canvas_at_pointer || layer.is_some_and(|layer| layer.id == edge_id))
        {
            self.zen_sidebar.revealed = true;
        }

        if !self.zen_sidebar.revealed {
            egui::Area::new(edge_id)
                .order(egui::Order::Middle)
                .fixed_pos(edge.min)
                .constrain(false)
                .fade_in(false)
                .show(ctx, |ui| {
                    let (rect, response) = ui.allocate_exact_size(edge.size(), Sense::hover());
                    response.on_hover_cursor(egui::CursorIcon::ResizeHorizontal);
                    ui.painter()
                        .rect_filled(rect, 0., Color32::from_white_alpha(26));
                    egui::Image::new(egui::include_image!("../../assets/img/stripes.png"))
                        .texture_options(egui::TextureOptions::NEAREST_REPEAT)
                        .uv(Rect::from_min_size(egui::Pos2::ZERO, rect.size() / 8.))
                        .paint_at(ui, rect);
                    ui.painter().vline(
                        rect.right() - 0.5,
                        rect.y_range(),
                        Stroke::new(1_f32, Color32::WHITE),
                    );
                });
        }
    }

    pub(super) fn toggle_zen_mode(&mut self, ctx: &egui::Context) {
        self.zen = !self.zen;
        self.zen_sidebar = ZenSidebar::default();
        self.settings.value["zenMode"] = self.zen.into();
        if let Err(error) = self.settings.save() {
            self.status = error.to_string();
            self.status_error = true;
        }
        let label = if self.zen {
            "Zen mode: ON"
        } else {
            "Zen mode: off"
        };
        self.quick_notification = Some((locale::tr_ctx(ctx, label).into(), None, Instant::now()));
    }

    fn layer_context_menu(
        &mut self,
        response: &egui::Response,
        index: usize,
        uid: i64,
        auto: bool,
        mac_context: bool,
    ) {
        if (response.secondary_clicked() || mac_context && response.clicked())
            && let Some(pos) = response.interact_pointer_pos()
        {
            self.layer_context_menu = Some((uid, pos));
        }
        let Some((_, anchor)) = self.layer_context_menu.filter(|(target, _)| *target == uid) else {
            return;
        };
        let ctx = &response.ctx;
        let screen = ctx.content_rect();
        let height = if auto { 118. } else { 79. };
        let y = if anchor.y >= screen.top() + screen.height() * 0.7 {
            anchor.y - 10. - height
        } else {
            anchor.y + 10.
        };
        let pos = egui::pos2(
            anchor.x,
            y.clamp(
                screen.top() + 16.,
                (screen.bottom() - 16. - height).max(screen.top() + 16.),
            ),
        );
        let id = response.id.with("layer-context");
        let menu = egui::Modal::new(id)
            .area(
                egui::Area::new(id)
                    .kind(egui::UiKind::Modal)
                    .order(egui::Order::Foreground)
                    .sense(Sense::hover())
                    .fixed_pos(pos)
                    .fade_in(false),
            )
            .backdrop_color(Color32::TRANSPARENT)
            .frame(
                panels::source_controls::context_menu_frame(&ctx.style())
                    .stroke(Stroke::new(1_f32, Color32::BLACK))
                    .shadow(egui::epaint::Shadow {
                        offset: [0, 4],
                        blur: 16,
                        spread: 0,
                        color: Color32::BLACK,
                    }),
            )
            .show(ctx, |ui| {
                let actions = [
                    (
                        "Toggle visibility",
                        egui::include_image!("../../assets/icons/visible.png"),
                        None,
                    ),
                    (
                        "Edit rules",
                        egui::include_image!("../../assets/icons/rule.svg"),
                        Some(Panel::Rules),
                    ),
                    (
                        "Edit layer settings",
                        egui::include_image!("../../assets/icons/edit.svg"),
                        Some(Panel::Layers),
                    ),
                ];
                let font = FontId::proportional(13.333);
                let rows = actions
                    .into_iter()
                    .filter(|(_, _, panel)| auto || *panel != Some(Panel::Rules))
                    .map(|(text, icon, panel)| {
                        let text = locale::tr(ui, text).to_uppercase();
                        let caption =
                            ui.painter()
                                .layout_no_wrap(text, font.clone(), theme::ACCENT);
                        (caption, icon, panel)
                    })
                    .collect::<Vec<_>>();
                let width = 70.
                    + rows
                        .iter()
                        .map(|(caption, _, _)| caption.size().x)
                        .fold(0., f32::max);
                ui.set_width(width);
                ui.spacing_mut().item_spacing.y = 0.;
                for (i, (caption, icon, panel)) in rows.into_iter().enumerate() {
                    let border = f32::from(i != 0);
                    let (rect, item) =
                        ui.allocate_exact_size(egui::vec2(width, 38. + border), Sense::click());
                    item.widget_info(|| {
                        egui::WidgetInfo::labeled(egui::WidgetType::Button, true, caption.text())
                    });
                    let painter = ui.painter();
                    let color = if item.hovered() {
                        painter.rect_filled(rect, 0., theme::ACCENT);
                        Color32::BLACK
                    } else {
                        theme::ACCENT
                    };
                    if i != 0 {
                        painter.hline(
                            rect.x_range(),
                            rect.top() + 0.5,
                            Stroke::new(1_f32, theme::MEDIUM),
                        );
                    }
                    let left = egui::pos2(rect.left() + 20., rect.top() + 8. + border);
                    egui::Image::new(icon)
                        .tint(Color32::WHITE.gamma_multiply(0.7))
                        .paint_at(ui, Rect::from_min_size(left, Vec2::splat(22.)));
                    let label = left + egui::vec2(30., 11. - caption.size().y * 0.5);
                    painter.galley_with_override_text_color(label, caption, color);
                    if item.clicked() {
                        if let Some(panel) = panel {
                            self.switch_layer(index);
                            self.definition_uid = Some(uid);
                            self.show_panel(Some(panel));
                        } else if let Some(key) = self.selected {
                            self.toggle_layer_visibility(
                                key,
                                index,
                                ui.input(|i| i.modifiers.shift),
                            );
                        }
                        self.layer_context_menu = None;
                        ui.ctx().request_repaint();
                    }
                }
            });
        if menu.should_close() || menu.backdrop_response.secondary_clicked() {
            self.layer_context_menu = None;
            ctx.request_repaint();
        }
    }

    pub(super) fn sidebar(&mut self, ctx: &egui::Context) {
        self.update_zen_sidebar(ctx);
        if self.project.is_none() || self.zen && (!self.zen_sidebar.revealed || self.world) {
            self.layer_visibility_press = None;
            self.layer_context_menu = None;
            return;
        }
        show_sidebar(ctx, self.zen, |ui| {
            if self.is_busy() || self.pending.is_some() {
                self.layer_context_menu = None;
                ui.disable();
                if self.save_failure.is_some() {
                    // The modal supplies the dimming for the sidebar.
                    ui.set_opacity(1.);
                }
            }
            ui.spacing_mut().item_spacing = Vec2::ZERO;
            ui.horizontal(|ui| {
                let (bar, _) =
                    ui.allocate_exact_size(egui::vec2(SIDEBAR_WIDTH - 2., 36.), Sense::hover());
                // Chromium lays out the ten equal flex children in 1/64-point units.
                let width = (bar.width() * 64. / 10.).floor() / 64.;
                let slot = |index| {
                    Rect::from_min_size(
                        bar.min + egui::vec2(index as f32 * width, 0.),
                        egui::vec2(width, bar.height()),
                    )
                };
                let linked = self.panel.filter(|panel| {
                    matches!(
                        panel,
                        Panel::Project
                            | Panel::World
                            | Panel::Level
                            | Panel::Layers
                            | Panel::Entities
                            | Panel::Enums
                            | Panel::Tilesets
                            | Panel::Help
                    )
                });
                let orange = (
                    Color32::from_rgb(227, 77, 0),
                    Color32::from_rgb(255, 93, 74),
                );
                let blue = (
                    Color32::from_rgb(42, 115, 171),
                    Color32::from_rgb(52, 143, 213),
                );
                for (index, (panel, image, fill, tip, keys)) in [
                    (
                        Panel::Project,
                        egui::include_image!("../../assets/icons/projectSettings.png"),
                        (
                            Color32::from_rgb(148, 78, 54),
                            Color32::from_rgb(191, 101, 70),
                        ),
                        "Project settings",
                        &["P"][..],
                    ),
                    (
                        Panel::World,
                        egui::include_image!("../../assets/icons/world.svg"),
                        blue,
                        "World view",
                        &[if self.settings.value["navigationKeys"] == "Zqsd" {
                            "²"
                        } else {
                            "~"
                        }][..],
                    ),
                    (
                        Panel::Level,
                        egui::include_image!("../../assets/icons/level.svg"),
                        blue,
                        "Current level settings",
                        &["C"][..],
                    ),
                    (
                        Panel::Layers,
                        egui::include_image!("../../assets/icons/layer.png"),
                        orange,
                        "Layers",
                        &["L"][..],
                    ),
                    (
                        Panel::Entities,
                        egui::include_image!("../../assets/icons/entity.png"),
                        orange,
                        "Entities",
                        &["E"][..],
                    ),
                    (
                        Panel::Enums,
                        egui::include_image!("../../assets/icons/enum.png"),
                        orange,
                        "Enums",
                        &["U"][..],
                    ),
                    (
                        Panel::Tilesets,
                        egui::include_image!("../../assets/icons/tile.png"),
                        orange,
                        "Tilesets",
                        &["T"][..],
                    ),
                    (
                        Panel::Help,
                        egui::include_image!("../../assets/icons/help.svg"),
                        (
                            Color32::from_rgb(73, 141, 85),
                            Color32::from_rgb(94, 171, 107),
                        ),
                        "Help and shortcuts",
                        &["H"][..],
                    ),
                    (
                        Panel::Settings,
                        egui::include_image!("../../assets/icons/settings.svg"),
                        (theme::LIGHT, Color32::from_rgb(115, 127, 157)),
                        "App settings",
                        &["F12"][..],
                    ),
                ]
                .into_iter()
                .enumerate()
                {
                    let state = match linked {
                        Some(active) if active == panel => ToolbarButtonState::Active,
                        Some(_) => ToolbarButtonState::Faded,
                        None => ToolbarButtonState::Normal,
                    };
                    let response = toolbar_button(ui, slot(index), image, fill, state, tip, keys);
                    if response.clicked() {
                        self.toggle_toolbar_panel(panel);
                    }
                    if panel == Panel::Project {
                        response.context_menu(|ui| {
                            if ui.button(locale::tr(ui, "New project")).clicked() {
                                self.request(ctx, Action::New);
                                ui.close();
                            }
                            if ui.button(locale::tr(ui, "Open…")).clicked() {
                                self.request(ctx, Action::Browse);
                                ui.close();
                            }
                            if ui.button(locale::tr(ui, "Save")).clicked() {
                                self.save(false);
                                ui.close();
                            }
                            if ui.button(locale::tr(ui, "Save as…")).clicked() {
                                self.save(true);
                                ui.close();
                            }
                            if ui.button(locale::tr(ui, "Export…")).clicked() {
                                self.show_panel(Some(Panel::Export));
                                ui.close();
                            }
                        });
                    } else if panel == Panel::World {
                        response.context_menu(|ui| {
                            if ui.button(locale::tr(ui, "World settings")).clicked() {
                                self.show_panel(Some(Panel::World));
                                ui.close();
                            }
                        });
                    }
                }
                if toolbar_button(
                    ui,
                    slot(9),
                    egui::include_image!("../../assets/icons/exit.svg"),
                    (CLOSE_TOOLBAR_COLOR, theme::LIGHT),
                    if linked.is_some() {
                        ToolbarButtonState::Faded
                    } else {
                        ToolbarButtonState::Normal
                    },
                    "Close project",
                    &["CTRL", "W"],
                )
                .clicked()
                {
                    self.request(ctx, Action::Close);
                }
            });
            // The close button can remove the project during this frame.
            if self.project.is_none() {
                return;
            }
            ui.add_space(4.);
            let project_iid = self.project.as_ref().unwrap().json["iid"]
                .as_str()
                .map(str::to_owned);
            let no_layers = self.project.as_ref().is_some_and(|p| {
                p.json["defs"]["layers"]
                    .as_array()
                    .is_none_or(Vec::is_empty)
            });
            egui::ScrollArea::vertical()
                .id_salt(("source-layer-list", project_iid.as_deref()))
                .max_height(250.)
                .auto_shrink([false, true])
                .scroll_source(egui::scroll_area::ScrollSource {
                    drag: false,
                    ..Default::default()
                })
                .show(ui, |ui| {
                    if no_layers {
                        empty_layer_list(ui);
                        return;
                    }
                    ui.set_min_height(100.);
                    self.layer_filter_controls(ui, project_iid.as_deref());
                    let shortcut_layers = self.visible_layer_indices(false);
                    let layers = self
                        .visible_layer_indices(true)
                        .into_iter()
                        .filter_map(|i| {
                            let p = self.project.as_ref()?;
                            let l = p.layer(self.selected?, i)?;
                            let uid = l["layerDefUid"].as_i64()?;
                            let d = p.layer_def(uid)?;
                            Some((
                                i,
                                uid,
                                text(l, "__identifier", locale::tr(ui, "Layer")).to_owned(),
                                text(l, "__type", "").to_owned(),
                                d.clone(),
                            ))
                        })
                        .collect::<Vec<_>>();
                    let mut visibility_rows = Vec::with_capacity(layers.len());
                    let dismissing_menu = self.layer_context_menu.is_some();
                    for (index, uid, name, kind, def) in &layers {
                        let active = self.layer == *index;
                        let visible = self
                            .selected
                            .is_some_and(|key| self.layer_visible(key, *index));
                        let alpha = if visible { 1. } else { 0.4 };
                        let custom = def["uiColor"].as_str().map(theme::color);
                        let (fill, foreground, rail) = match (custom, active) {
                            (Some(color), true) => {
                                let luminance = 299 * u32::from(color.r())
                                    + 587 * u32::from(color.g())
                                    + 114 * u32::from(color.b());
                                let foreground = if luminance >= 127_500 {
                                    Color32::BLACK
                                } else {
                                    Color32::WHITE
                                };
                                (color, foreground, Some(Color32::WHITE))
                            }
                            (Some(color), false) => (
                                theme::DARK.lerp_to_gamma(color, 0.5),
                                color.lerp_to_gamma(Color32::WHITE, 0.3),
                                Some(color),
                            ),
                            (None, true) => (theme::ACCENT, Color32::BLACK, None),
                            (None, false) => (theme::MEDIUM, Color32::WHITE, None),
                        };
                        let (rect, response) = ui.allocate_exact_size(
                            egui::vec2(ui.available_width(), 31.),
                            Sense::click(),
                        );
                        let row = Rect::from_min_max(rect.min, rect.max - egui::vec2(2., 0.));
                        let fade = |color| theme::DARK.lerp_to_gamma(color, alpha);
                        if active && custom.is_none() {
                            let shadow = egui::epaint::Shadow {
                                offset: [0, 0],
                                blur: 4,
                                spread: 0,
                                color: Color32::from_rgb(170, 0, 0).gamma_multiply(alpha),
                            };
                            ui.painter().add(shadow.as_shape(row, 0.));
                        }
                        ui.painter().rect_filled(rect, 0., fade(fill));
                        if let Some(color) = rail {
                            let rail = Rect::from_min_max(
                                egui::pos2(row.right() - 4., row.top()),
                                row.max,
                            );
                            ui.painter().rect_filled(rail, 0., fade(color));
                        }
                        if active && custom.is_some() {
                            ui.painter().rect_stroke(
                                row.shrink(1.),
                                0.,
                                Stroke::new(1_f32, fade(Color32::WHITE)),
                                StrokeKind::Inside,
                            );
                        } else if active {
                            ui.painter().rect_stroke(
                                row,
                                0.,
                                Stroke::new(1_f32, fade(Color32::from_rgb(255, 223, 77))),
                                StrokeKind::Inside,
                            );
                        } else if ui.is_enabled() && ui.rect_contains_pointer(row) {
                            ui.painter().rect_stroke(
                                row,
                                0.,
                                Stroke::new(2_f32, fade(fill.lerp_to_gamma(Color32::WHITE, 0.65))),
                                StrokeKind::Inside,
                            );
                        }
                        let opacity = ui.opacity();
                        ui.multiply_opacity(alpha);
                        if let Some(shortcut) =
                            shortcut_layers.iter().take(10).position(|i| i == index)
                        {
                            ui.painter().text(
                                rect.left_center() + egui::vec2(8., 0.),
                                Align2::LEFT_CENTER,
                                format!("F{}", shortcut + 1),
                                FontId::new(10.667, egui::FontFamily::Name("body-bold".into())),
                                foreground.gamma_multiply(0.5),
                            );
                        }
                        let image = match kind.as_str() {
                            "Entities" => egui::include_image!("../../assets/icons/entity.png"),
                            "IntGrid" => egui::include_image!("../../assets/icons/intGrid.png"),
                            "AutoLayer" => {
                                egui::include_image!("../../assets/icons/autoLayer.svg")
                            }
                            _ => egui::include_image!("../../assets/icons/tile.png"),
                        };
                        egui::Image::new(image).paint_at(
                            ui,
                            Rect::from_center_size(
                                rect.left_center() + egui::vec2(41.1, 0.),
                                Vec2::splat(20.8),
                            ),
                        );
                        ui.painter().text(
                            rect.left_center() + egui::vec2(59.5, 0.),
                            Align2::LEFT_CENTER,
                            name,
                            FontId::proportional(13.333),
                            foreground,
                        );
                        let eye =
                            Rect::from_min_max(egui::pos2(row.right() - 28.8, row.top()), row.max);
                        visibility_rows.push((*index, row, eye));
                        let eye_response =
                            ui.interact(eye, ui.id().with(("visible", uid)), Sense::click());
                        if eye_response.hovered() {
                            ui.painter().rect_filled(eye, 3., Color32::WHITE);
                        }
                        egui::Image::new(if visible {
                            egui::include_image!("../../assets/icons/visible.png")
                        } else {
                            egui::include_image!("../../assets/icons/hidden.png")
                        })
                        .paint_at(ui, Rect::from_center_size(eye.center(), Vec2::splat(20.8)));
                        let auto = kind == "AutoLayer"
                            || def["autoRuleGroups"]
                                .as_array()
                                .is_some_and(|a| !a.is_empty());
                        let rule_rect = Rect::from_center_size(
                            rect.right_center() - egui::vec2(51.0, 0.0),
                            egui::vec2(43.0, 26.0),
                        );
                        let mut menu_response = response.union(eye_response);
                        let mac_context =
                            cfg!(target_os = "macos") && ui.input(|input| input.modifiers.ctrl);
                        if auto {
                            let r = ui.interact(
                                rule_rect,
                                ui.id().with(("layer-rules", uid)),
                                Sense::click(),
                            );
                            ui.painter().rect_filled(
                                rule_rect,
                                3.,
                                if r.hovered() {
                                    theme::LIGHT
                                } else {
                                    Color32::from_rgb(84, 93, 115)
                                },
                            );
                            ui.painter().text(
                                rule_rect.center(),
                                Align2::CENTER_CENTER,
                                locale::tr(ui, "RULES"),
                                FontId::proportional(11.),
                                Color32::WHITE,
                            );
                            if r.clicked() && !mac_context {
                                self.switch_layer(*index);
                                self.definition_uid = Some(*uid);
                                self.show_panel(Some(Panel::Rules));
                            }
                            menu_response = menu_response.union(r);
                        }
                        if response.clicked() && !mac_context {
                            let pointer =
                                ui.input(|i| i.pointer.interact_pos().unwrap_or_default());
                            let on_control =
                                eye.contains(pointer) || (auto && rule_rect.contains(pointer));
                            if !on_control {
                                self.switch_layer(*index);
                            }
                        }
                        ui.set_opacity(opacity);
                        response.on_hover_text(text(def, "doc", ""));
                        self.layer_context_menu(&menu_response, *index, *uid, auto, mac_context);
                        ui.add_space(1.);
                    }
                    if !dismissing_menu {
                        self.handle_layer_visibility(ui, &visibility_rows);
                    }
                });
            let kind = self
                .selected
                .and_then(|key| self.project.as_ref()?.layer(key, self.layer))
                .map(|layer| text(layer, "__type", "").to_owned())
                .unwrap_or_default();
            sidebar_guide(ui, &kind, no_layers);
            self.library_palette(ctx, ui);
        });
    }

    pub(super) fn world_switcher(&mut self, ctx: &egui::Context) {
        let Some(project) = &self.project else {
            return;
        };
        let worlds = canvas::values(&project.json["worlds"]);
        if worlds.len() <= 1 {
            return;
        }
        let origin = ctx.content_rect().left_top() + egui::vec2(SIDEBAR_WIDTH + 2., 4.);
        let mut requested = None;
        egui::Area::new(egui::Id::new("world-switcher"))
            .fixed_pos(origin)
            .order(egui::Order::Foreground)
            .constrain(false)
            .show(ctx, |ui| {
                if self.is_busy() || self.pending.is_some() {
                    ui.disable();
                }
                egui::Frame::new()
                    .fill(theme::DARK)
                    .corner_radius(3)
                    .inner_margin(2)
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.x = 2.;
                        ui.horizontal(|ui| {
                            for (index, world) in worlds.iter().enumerate() {
                                let name = text(world, "identifier", "World");
                                let active = self.active_world == Some(index);
                                let (rect, response) =
                                    ui.allocate_exact_size(egui::vec2(42., 26.), Sense::click());
                                let (fill, border, text_color) = if active {
                                    (theme::ACCENT, theme::LIGHT, Color32::BLACK)
                                } else if response.hovered() {
                                    (Color32::TRANSPARENT, Color32::WHITE, Color32::WHITE)
                                } else {
                                    (Color32::TRANSPARENT, theme::LIGHT, theme::ACCENT)
                                };
                                response.widget_info(|| {
                                    egui::WidgetInfo::selected(
                                        egui::WidgetType::SelectableLabel,
                                        ui.is_enabled(),
                                        active,
                                        name,
                                    )
                                });
                                ui.painter().rect(
                                    rect,
                                    3.,
                                    fill,
                                    Stroke::new(1_f32, border),
                                    StrokeKind::Inside,
                                );
                                ui.painter().with_clip_rect(rect.shrink(1.)).text(
                                    rect.center(),
                                    Align2::CENTER_CENTER,
                                    short_world_name(name),
                                    FontId::new(12., egui::FontFamily::Name("heading".into())),
                                    text_color,
                                );
                                if response
                                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                                    .clicked()
                                {
                                    requested = Some(index);
                                }
                            }
                        });
                    });
            });
        if let Some(index) = requested {
            self.switch_world(Some(index), !self.world);
        }
    }

    pub(super) fn canvas_toolbar(&mut self, ctx: &egui::Context, rect: Rect) {
        if self.world {
            self.world_layers_hud(ctx);
        }
        if self.project.is_none() || self.zen && (!self.zen_sidebar.revealed || self.world) {
            return;
        }
        let tiles = self
            .selected
            .and_then(|key| self.project.as_ref()?.layer(key, self.layer))
            .is_some_and(|layer| layer["__type"] == "Tiles");
        egui::Area::new(egui::Id::new("editing-options"))
            .fixed_pos(rect.left_top() + egui::vec2(0.0, 46.0))
            .order(egui::Order::Foreground)
            .fade_in(false)
            .interactable(self.camera_drag.is_none())
            .show(ctx, |parent| {
                let mut frame = egui::Frame::new()
                    .fill(theme::DARK)
                    .inner_margin(egui::Margin { left: 0, right: 3, top: 3, bottom: 3 })
                    .corner_radius(egui::CornerRadius { nw: 0, ne: 4, sw: 0, se: 4 })
                    .begin(parent);
                let ui = &mut frame.content_ui;
                ui.spacing_mut().item_spacing = egui::vec2(0., 1.);
                ui.set_width(34.);
                canvas_option_heading(ui, "Visuals", Color32::from_rgb(101, 148, 255))
                .on_hover_ui(|ui| {
                    let cache = self.assets.stats();
                    ui.set_max_width(ui.spacing().tooltip_width);
                    ui.label(locale::format(
                        ui,
                        "{zoom}% · {cells} visible cells · {cpu} ms CPU\n{textures} textures · {memory} MiB · {pending} pending",
                        &[
                            ("zoom", &format!("{:.0}", self.viewport.zoom * 100.0)),
                            ("cells", &self.visible_tiles.to_string()),
                            ("cpu", &format!("{:.2}", self.frame_time)),
                            ("textures", &cache.textures.to_string()),
                            ("memory", &format!("{:.1}", cache.bytes as f64 / 1_048_576.0)),
                            ("pending", &cache.pending.to_string()),
                        ],
                    ));
                });
                for (source, enabled, tip, keys, action) in [
                    (
                        egui::include_image!("../../assets/icons/zen.svg"),
                        self.zen,
                        "**Zen mode** This mode automatically hides the left panel to give more editing space.",
                        &["TAB"][..],
                        0,
                    ),
                    (
                        if self.grid {
                            egui::include_image!("../../assets/icons/gridOn.svg")
                        } else {
                            egui::include_image!("../../assets/icons/gridOff.svg")
                        },
                        self.grid,
                        "**Grid** Show the layer grid. In supported layer types, hiding the grid will also allow free positioning of elements.",
                        &["G"][..],
                        1,
                    ),
                    (
                        if self.details {
                            egui::include_image!("../../assets/icons/showDetailsOn.svg")
                        } else {
                            egui::include_image!("../../assets/icons/showDetailsOff.svg")
                        },
                        self.details,
                        "**Show/hide details** When enabled, shows all elements in layers (tiles, entities, intGrid etc.), or only tiles otherwise.",
                        if cfg!(target_os = "macos") { &["SHIFT", "H"][..] } else { &["CTRL", "H"][..] },
                        2,
                    ),
                    (
                        egui::include_image!("../../assets/icons/singleLayer.svg"),
                        self.single_layer,
                        "**Single layer mode** Enable this to fade all other layers, and focus on active layer only. NOTE: this will also affect the Selection tool (ALT+click).",
                        &["SHIFT", "A"][..],
                        3,
                    ),
                    (
                        egui::include_image!("../../assets/icons/autoLayer.svg"),
                        self.render_auto,
                        "**Render auto-layers** Enable or disable the rendering of tiles in all Auto-Layers. If disabled, only the underlying IntGrid values will be displayed.",
                        &["SHIFT", "R"][..],
                        4,
                    ),
                    (
                        egui::include_image!("../../assets/icons/enum.png"),
                        self.tile_tags,
                        "**Show/hide enum tags** When enabled, shows a color overlay on tiles that have an Enum tag attached to them (see the Tilesets panel to tag tiles using Enums).\\nOnly applies to Tiles layers and Auto-layers.",
                        &[][..],
                        5,
                    ),
                ] {
                    if canvas_option(ui, source, enabled, action != 5 || tiles, true, tip, keys)
                    .clicked()
                    {
                        match action {
                            0 => {
                                self.toggle_zen_mode(ctx);
                                self.zen_sidebar.revealed = self.zen;
                            }
                            1 => self.set_grid(!self.grid),
                            2 => self.set_details(!self.details),
                            3 => self.set_single_layer_mode(!self.single_layer),
                            4 => self.set_auto_layer_rendering(!self.render_auto, false),
                            _ => self.set_tile_enum_overlays(!self.tile_tags),
                        }
                    }
                }
                ui.add_space(8.);
                canvas_option_heading(ui, "Editing", Color32::from_rgb(207, 221, 64));
                let select_empty = self.settings.bool("emptySpaceSelection");
                if canvas_option(
                    ui,
                    if select_empty {
                        egui::include_image!("../../assets/icons/emptySpaceOn.png")
                    } else {
                        egui::include_image!("../../assets/icons/emptySpaceOff.png")
                    },
                    select_empty,
                    true,
                    false,
                    "**Select empty spaces** If this option is enabled when you select a level area with ALT+SHIFT+click, the selection will also include EMPTY grid cells.",
                    &["SHIFT", "E"],
                )
                .clicked()
                {
                    self.set_empty_selection(!select_empty);
                }
                if canvas_option(
                    ui,
                    egui::include_image!("../../assets/icons/tile.png"),
                    self.tile_stacking,
                    tiles,
                    false,
                    "**Tile stacking** Only applies to Tiles layers. If this option is enabled, you will be able to stack multiple tiles in a single grid cell.",
                    &["SHIFT", "T"],
                )
                .clicked()
                {
                    self.set_tile_stacking(!self.tile_stacking);
                }
                frame.end(parent);
            });
    }

    fn world_layers_hud(&mut self, ctx: &egui::Context) {
        let Some((min, max)) = self.world_depth_bounds().filter(|(min, max)| min < max) else {
            return;
        };
        const TITLE_HEIGHT: f32 = 27.;
        const ROW_HEIGHT: f32 = 28.;
        const ROW_STEP: f32 = 20.;
        const LABEL_FONT: f32 = 13.333;
        let row_count = max.abs_diff(min).saturating_add(1);
        let panning = self.camera_drag.is_some();
        let mut next_depth = None;
        // The source World wrapper contributes its one-pixel seam before the 40px offset.
        let left = panels::source_panel_width(Panel::World) + 41.;
        egui::Area::new(egui::Id::new("world-depths"))
            .fixed_pos(ctx.content_rect().left_top() + egui::vec2(left, 46.))
            .constrain(false)
            .order(egui::Order::Foreground)
            .interactable(!panning)
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(theme::DARK)
                    .inner_margin(3)
                    .corner_radius(4)
                    .shadow(egui::epaint::Shadow {
                        offset: [0, 4],
                        blur: 2,
                        spread: 0,
                        color: Color32::from_black_alpha(102),
                    })
                    .show(ui, |ui| {
                        let font = FontId::proportional(LABEL_FONT);
                        let label_width = [min, max]
                            .into_iter()
                            .map(|depth| ui.painter().layout_no_wrap(depth.to_string(), font.clone(), theme::LIGHT).size().x)
                            .fold(LABEL_FONT, f32::max) + 8.;
                        ui.set_width(103_f32.max(61. + label_width));
                        egui::ScrollArea::vertical()
                            .max_height((ctx.content_rect().height() * 0.7 - 6.).max(1.))
                            .auto_shrink([false, true])
                            .scroll_source(egui::scroll_area::ScrollSource {
                                scroll_bar: !panning,
                                drag: false,
                                mouse_wheel: !panning,
                            })
                            .show_viewport(ui, |ui, viewport| {
                                let origin = ui.max_rect().min;
                                let width = ui.available_width();
                                ui.set_min_size(egui::vec2(width, TITLE_HEIGHT + ROW_HEIGHT - ROW_STEP + ROW_STEP * row_count as f32));
                                if viewport.min.y <= TITLE_HEIGHT {
                                    let title_rect = Rect::from_min_size(origin + egui::vec2(0., 3.), egui::vec2(width - 16., 16.));
                                    ui.put(title_rect, egui::Label::new(RichText::new(locale::tr(ui, "World layers"))
                                        .strong().size(11.733).color(theme::LIGHT)));
                                    let hint = Rect::from_min_size(title_rect.right_top(), Vec2::splat(16.));
                                    let help_color = Color32::from_rgb(94, 171, 107);
                                    ui.painter().circle_stroke(hint.center(), 7., Stroke::new(2_f32, help_color));
                                    ui.painter().text(hint.center(), Align2::CENTER_CENTER, "?", FontId::proportional(11.), help_color);
                                    if !panning {
                                        let response = ui.interact(hint, ui.id().with("depth-help"), Sense::hover())
                                            .on_hover_cursor(egui::CursorIcon::Help);
                                        source_tooltip(&response, "World layers are created or removed automatically, as you arrange your levels using the \"Change depth\" buttons (ie. move level above/beneath).", &[], true);
                                    }
                                }

                                let start = ((viewport.min.y - TITLE_HEIGHT - ROW_HEIGHT) / ROW_STEP).floor().max(0.) as u64;
                                let end = (((viewport.max.y - TITLE_HEIGHT) / ROW_STEP).ceil() as u64).min(row_count);
                                let rows = start..end;
                                let active_index = (min..=max).contains(&self.world_depth)
                                    .then(|| max.abs_diff(self.world_depth))
                                    .filter(|index| rows.contains(index));
                                // Paint the selected layer above both of its overlapping neighbors.
                                for index in rows.filter(|index| Some(*index) != active_index).chain(active_index) {
                                    let depth = (i128::from(max) - i128::from(index)) as i64;
                                    let active = depth == self.world_depth;
                                    let row = Rect::from_min_size(origin + egui::vec2(0., TITLE_HEIGHT + ROW_STEP * index as f32),
                                        egui::vec2(width, ROW_HEIGHT));
                                    let mut hovered = false;
                                    if !active && !panning {
                                        let hit_rect = if depth == min { row } else {
                                            Rect::from_min_size(row.min, egui::vec2(width, ROW_STEP))
                                        };
                                        let response = ui.interact(hit_rect, ui.id().with(("depth", depth)), Sense::click())
                                            .on_hover_cursor(egui::CursorIcon::PointingHand);
                                        hovered = response.hovered();
                                        if response.clicked() {
                                            next_depth = Some(depth);
                                        }
                                    }
                                    if hovered {
                                        ui.painter().rect_filled(row.translate(egui::vec2(2., 0.)), 4., theme::MEDIUM);
                                    }
                                    paint_world_depth_row(ui, row, depth, min..=max, active, hovered, &font);
                                }
                            });
                    });
            });
        if let Some(depth) = next_depth {
            self.select_world_depth(depth);
        }
    }
}

fn paint_world_depth_row(
    ui: &egui::Ui,
    row: Rect,
    depth: i64,
    depths: std::ops::RangeInclusive<i64>,
    active: bool,
    hovered: bool,
    font: &FontId,
) {
    let icon = if depth == *depths.start() {
        egui::include_image!("../../assets/icons/worldLayer_top.svg")
    } else {
        egui::include_image!("../../assets/icons/worldLayer_below.svg")
    };
    let tint = if active || hovered {
        theme::ACCENT
    } else if depth == *depths.end() {
        Color32::WHITE.gamma_multiply(0.5)
    } else {
        Color32::WHITE
    };
    let left = row.left() + if hovered { 6. } else { 4. };
    egui::Image::new(icon)
        .tint(tint)
        .uv(Rect {
            min: egui::pos2(0., 1.),
            max: egui::pos2(1., 0.),
        })
        .paint_at(
            ui,
            Rect::from_center_size(egui::pos2(left + 25., row.center().y), egui::vec2(50., 25.)),
        );
    let color = if active {
        Color32::BLACK
    } else if hovered {
        theme::TEXT
    } else {
        theme::LIGHT
    };
    let label = ui
        .painter()
        .layout_no_wrap(depth.to_string(), font.clone(), color);
    let badge = Rect::from_min_size(
        egui::pos2(left + 53., row.center().y - 7.5),
        egui::vec2(label.size().x.max(font.size) + 8., 15.),
    );
    if active {
        ui.painter().rect_filled(badge, 4., theme::ACCENT);
    }
    ui.painter()
        .galley(badge.center() - label.size() * 0.5, label, color);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zen_mode_reveals_the_sidebar_over_the_canvas_and_hides_it_on_return() {
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let mut project = Project::new();
        project.add_layer("Zen_probe", "IntGrid", 16).unwrap();
        let mut app = EditorApp::empty();
        app.set_project(project);
        app.zen = true;
        let screen = Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1440., 900.));
        let frame = |app: &mut EditorApp, time, pointer: Option<egui::Pos2>| {
            ctx.run(
                egui::RawInput {
                    screen_rect: Some(screen),
                    time: Some(time),
                    events: pointer.into_iter().map(egui::Event::PointerMoved).collect(),
                    ..Default::default()
                },
                |ctx| eframe::App::update(app, ctx, &mut eframe::Frame::_new_kittest()),
            )
        };
        let visible = |output: &egui::FullOutput| {
            output.shapes.iter().any(|shape| {
                matches!(&shape.shape, egui::Shape::Text(text) if text.galley.text() == "Zen_probe")
            })
        };
        assert!(!visible(&frame(&mut app, 0., Some(egui::pos2(730., 400.)))));
        let before = app.viewport.clone();
        frame(&mut app, 0.05, Some(egui::pos2(8., 20.)));
        let shown = frame(&mut app, 0.08, None);
        assert!(
            visible(&shown),
            "hovering the left edge reveals the existing sidebar"
        );
        let canvas = ctx.data(|data| {
            data.get_temp::<Rect>(egui::Id::new("entity-instance-inspector").with("canvas"))
        });
        assert_eq!(canvas, Some(screen));
        assert_eq!(app.viewport.center, before.center);
        assert_eq!(app.viewport.zoom, before.zoom);

        frame(&mut app, 0.2, Some(egui::pos2(390., 400.)));
        assert!(visible(&frame(&mut app, 0.7, None)));
        assert!(!visible(&frame(&mut app, 1.25, None)));
        frame(&mut app, 1.3, Some(egui::pos2(8., 20.)));
        assert!(visible(&frame(&mut app, 1.33, None)));
        frame(&mut app, 1.4, Some(egui::pos2(730., 400.)));
        assert!(!visible(&frame(&mut app, 1.7, None)));
    }

    #[test]
    fn zen_toolbar_keeps_the_sidebar_revealed_so_the_button_can_turn_it_off() {
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let mut app = EditorApp::empty();
        app.set_project(Project::new());
        let button = egui::pos2(317., 81.);
        app_frame(&mut app, &ctx, vec![egui::Event::PointerMoved(button)]);
        app_frame(&mut app, &ctx, vec![]);
        app_click(&mut app, &ctx, button);
        app_frame(&mut app, &ctx, vec![]);
        assert!(app.zen);
        assert!(app.zen_sidebar.revealed);
        assert_eq!(app.settings.value["zenMode"], true);

        app_click(&mut app, &ctx, button);
        assert!(!app.zen);
        assert!(!app.zen_sidebar.revealed);
        assert_eq!(app.settings.value["zenMode"], false);
    }

    #[test]
    fn losing_window_focus_collapses_the_revealed_zen_sidebar() {
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let mut app = EditorApp::empty();
        app.set_project(Project::new());
        app.zen = true;
        app_frame(
            &mut app,
            &ctx,
            vec![egui::Event::PointerMoved(egui::pos2(8., 20.))],
        );
        assert!(app.zen_sidebar.revealed);

        app_frame(&mut app, &ctx, vec![egui::Event::PointerGone]);
        assert!(app.zen_sidebar.revealed);
        app_frame(&mut app, &ctx, vec![egui::Event::WindowFocused(false)]);
        assert!(app.zen);
        assert!(!app.zen_sidebar.revealed);
    }

    #[test]
    fn exiting_zen_mode_with_tab_keeps_editor_shortcuts_available() {
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let mut project = Project::new();
        project.add_layer("Ground", "IntGrid", 16).unwrap();
        let mut app = EditorApp::empty();
        app.set_project(project);
        app.zen = true;
        app_frame(
            &mut app,
            &ctx,
            vec![egui::Event::PointerMoved(egui::pos2(730., 400.))],
        );
        let key = |key, pressed| egui::Event::Key {
            key,
            physical_key: Some(key),
            pressed,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        app_frame(&mut app, &ctx, vec![key(Key::Tab, true)]);
        assert!(!app.zen);
        app_frame(&mut app, &ctx, vec![key(Key::Tab, false)]);
        app_frame(&mut app, &ctx, vec![key(Key::Backtick, true)]);
        assert!(
            app.world,
            "Tab must not focus a sidebar widget when it toggles Zen mode"
        );
        app_frame(&mut app, &ctx, vec![key(Key::Tab, true)]);
        assert!(app.zen);
        assert_eq!(app.panel, Some(Panel::World));
    }

    fn app_frame(
        app: &mut EditorApp,
        ctx: &egui::Context,
        events: Vec<egui::Event>,
    ) -> egui::FullOutput {
        let mut input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1440., 900.),
            )),
            focused: true,
            modifiers: events
                .iter()
                .find_map(|event| match event {
                    egui::Event::Key { modifiers, .. } => Some(*modifiers),
                    _ => None,
                })
                .unwrap_or_default(),
            events,
            ..Default::default()
        };
        eframe::App::raw_input_hook(app, ctx, &mut input);
        let output = ctx.run(input, |ctx| {
            eframe::App::update(app, ctx, &mut eframe::Frame::_new_kittest())
        });
        assert!(app.crash.is_none(), "{:?}", app.crash);
        output
    }

    fn app_click(app: &mut EditorApp, ctx: &egui::Context, pos: egui::Pos2) {
        for pressed in [true, false] {
            app_frame(
                app,
                ctx,
                vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: Default::default(),
                    },
                ],
            );
        }
    }

    fn source_key_texture(ctx: &egui::Context) -> egui::TextureId {
        let image = egui::Image::new(egui::include_image!("../../assets/img/keycap.svg"))
            .texture_options(egui::TextureOptions::LINEAR)
            .fit_to_exact_size(Vec2::splat(32.));
        let until = Instant::now() + std::time::Duration::from_secs(1);
        loop {
            if let Some(id) = image
                .load_for_size(ctx, Vec2::splat(32.))
                .unwrap()
                .texture_id()
            {
                return id;
            }
            assert!(Instant::now() < until, "source keycap asset did not load");
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    }

    #[test]
    fn rendered_shortcut_background_matches_source_css_translucency() {
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let until = Instant::now() + std::time::Duration::from_secs(1);
        loop {
            let output = ctx.run(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let (rect, _) = ui.allocate_exact_size(egui::vec2(28., 23.5), Sense::hover());
                    paint_shortcut(ui, rect, "W", true);
                });
            });
            if let Some(texture) = output.shapes.iter().find_map(|shape| match &shape.shape {
                egui::Shape::Mesh(mesh) if mesh.vertices.len() == 36 => Some(mesh.texture_id),
                _ => None,
            }) {
                let delta = output
                    .textures_delta
                    .set
                    .iter()
                    .find_map(|(id, delta)| (*id == texture).then_some(delta))
                    .expect("the painted keycap must upload its real background texture");
                assert_eq!(
                    delta.options.magnification,
                    egui::TextureFilter::Linear,
                    "rounded key edges must stay smooth when the mesh stretches"
                );
                let egui::ImageData::Color(image) = &delta.image;
                let center = image[(image.width() / 2, image.height() / 3)].to_srgba_unmultiplied();
                for (actual, expected) in center.into_iter().zip([225u8, 214, 187, 140]) {
                    assert!(
                        actual.abs_diff(expected) <= 2,
                        "the source key background must be translucent beige: {center:?}"
                    );
                }
                let border =
                    image[(image.width() / 2, image.height() * 3 / 4)].to_srgba_unmultiplied();
                assert!(
                    border[3].abs_diff(203) <= 2,
                    "the source 4px bottom border overlays the translucent background: {border:?}"
                );
                break;
            }
            assert!(Instant::now() < until, "keycap did not render");
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    }

    #[test]
    fn toolbar_hover_shows_source_tooltip_with_shortcut_keycaps() {
        for (pos, label, keys) in [
            (egui::pos2(45., 18.), "World view", vec!["~"]),
            (
                egui::pos2(285., 18.),
                "Close project",
                vec![
                    if cfg!(target_os = "macos") {
                        "⌘"
                    } else {
                        "CTRL"
                    },
                    "W",
                ],
            ),
        ] {
            let ctx = egui::Context::default();
            theme::configure(&ctx);
            ctx.style_mut(|style| {
                style.interaction.tooltip_delay = 0.;
                style.animation_time = 0.;
            });
            let texture = source_key_texture(&ctx);
            let mut app = EditorApp::empty();
            app.set_project(Project::new());
            app_frame(&mut app, &ctx, vec![]);
            let mut output = None;
            for _ in 0..4 {
                output = Some(app_frame(
                    &mut app,
                    &ctx,
                    vec![egui::Event::PointerMoved(pos)],
                ));
            }
            let output = output.unwrap();
            let tooltip = output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) if text.galley.text() == label => {
                        Some(Rect::from_min_size(text.pos, text.galley.size()))
                    }
                    _ => None,
                })
                .unwrap_or_else(|| panic!("hover must show the source label {label:?}"));
            assert!(
                output.shapes.iter().any(|shape| {
                    let is_source_popup = |shape: &egui::Shape| {
                        matches!(shape, egui::Shape::Rect(rect)
                        if rect.fill == Color32::from_rgb(60, 97, 72)
                            && rect.rect.contains_rect(tooltip))
                    };
                    is_source_popup(&shape.shape)
                        || matches!(&shape.shape, egui::Shape::Vec(shapes)
                        if shapes.iter().any(is_source_popup))
                }),
                "the tooltip must use the source green popup"
            );
            for key in &keys {
                assert!(output.shapes.iter().any(|shape| {
                    matches!(&shape.shape, egui::Shape::Text(text) if text.galley.text() == *key)
                }), "missing shortcut symbol {key:?}");
            }
            assert_eq!(
                output
                    .shapes
                    .iter()
                    .filter(|shape| {
                        matches!(&shape.shape, egui::Shape::Mesh(mesh)
                        if mesh.texture_id == texture && mesh.vertices.len() == 36)
                    })
                    .count(),
                keys.len(),
                "each shortcut key must paint the CSS keycap background as nine slices"
            );
        }
    }

    #[test]
    fn sidebar_guide_shows_source_shift_symbol_on_the_same_keycap_asset() {
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let texture = source_key_texture(&ctx);
        let mut app = EditorApp::empty();
        app.set_project(
            Project::load(
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("examples/original/Typical_TopDown_example.ldtk"),
            )
            .unwrap(),
        );
        app.layer = app
            .project
            .as_ref()
            .unwrap()
            .level(app.selected.unwrap())
            .unwrap()["layerInstances"]
            .as_array()
            .unwrap()
            .iter()
            .position(|layer| layer["__type"] == "Tiles")
            .unwrap();
        let mut output = None;
        for _ in 0..3 {
            output = Some(app_frame(&mut app, &ctx, vec![]));
        }
        let output = output.unwrap();
        assert!(
            output.shapes.iter().any(|shape| {
                matches!(&shape.shape, egui::Shape::Text(text) if text.galley.text() == "⇧")
            }),
            "the rectangle guide must show the source Shift symbol"
        );
        assert_eq!(
            output
                .shapes
                .iter()
                .filter(|shape| {
                    matches!(&shape.shape, egui::Shape::Mesh(mesh)
                if mesh.texture_id == texture && mesh.vertices.len() == 36)
                })
                .count(),
            2,
            "the rectangle and picker guides must use source keycaps"
        );
    }

    #[test]
    fn close_project_button_returns_home_without_panicking_in_level_and_world_view() {
        for world in [false, true] {
            let ctx = egui::Context::default();
            theme::configure(&ctx);
            let mut app = EditorApp::empty();
            app.set_project(Project::new());
            app.world = world;
            app_frame(&mut app, &ctx, vec![]);
            app_click(&mut app, &ctx, egui::pos2(285., 18.));
            app_frame(&mut app, &ctx, vec![]);
            assert!(app.project.is_none(), "close button must return to Home");
            assert!(app.selected.is_none() && app.pending.is_none());
        }
    }

    #[test]
    fn close_project_button_preserves_unsaved_save_discard_and_cancel() {
        for choice in ["YES", "NO", "CANCEL"] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("project.ldtk");
            let mut project = Project::new();
            project.save(&path).unwrap();
            let before = std::fs::read(&path).unwrap();
            let key = project.level_keys()[0];
            project.level_mut(key).unwrap()["identifier"] = "Unsaved".into();
            let ctx = egui::Context::default();
            theme::configure(&ctx);
            let mut app = EditorApp::empty();
            app.repaint = Some(ctx.clone());
            app.set_project(project);
            app.dirty = true;
            app_frame(&mut app, &ctx, vec![]);
            app_click(&mut app, &ctx, egui::pos2(285., 18.));
            assert!(app.project.is_some() && app.dirty);
            assert!(matches!(app.pending, Some(Action::Close)));
            let output = app_frame(&mut app, &ctx, vec![]);
            let pos = output
                .shapes
                .iter()
                .find_map(|s| match &s.shape {
                    egui::Shape::Text(t) if t.galley.text() == choice => {
                        Some(t.pos + t.galley.size() * 0.5)
                    }
                    _ => None,
                })
                .unwrap_or_else(|| panic!("missing unsaved choice {choice}"));
            app_click(&mut app, &ctx, pos);
            let until = Instant::now() + std::time::Duration::from_secs(3);
            while (app.is_busy() || app.pending.is_some()) && Instant::now() < until {
                app_frame(&mut app, &ctx, vec![]);
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
            app_frame(&mut app, &ctx, vec![]);
            assert!(app.pending.is_none() && !app.is_busy(), "{choice}");
            if choice == "CANCEL" {
                assert!(app.project.is_some() && app.dirty);
                assert_eq!(
                    app.project.as_ref().unwrap().level(key).unwrap()["identifier"],
                    "Unsaved"
                );
            } else {
                assert!(app.project.is_none(), "{choice} must close the project");
            }
            if choice == "YES" {
                assert_eq!(
                    Project::load(&path).unwrap().level(key).unwrap()["identifier"],
                    "Unsaved"
                );
            } else {
                assert_eq!(std::fs::read(path).unwrap(), before);
            }
        }
    }

    #[test]
    fn native_window_close_keeps_the_project_and_guards_unsaved_changes() {
        for dirty in [false, true] {
            let ctx = egui::Context::default();
            theme::configure(&ctx);
            let mut app = EditorApp::empty();
            app.set_project(Project::new());
            app.dirty = dirty;
            let mut input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1440., 900.),
                )),
                focused: true,
                ..Default::default()
            };
            input
                .viewports
                .get_mut(&egui::ViewportId::ROOT)
                .unwrap()
                .events
                .push(egui::ViewportEvent::Close);
            eframe::App::raw_input_hook(&mut app, &ctx, &mut input);
            let output = ctx.run(input, |ctx| {
                eframe::App::update(&mut app, ctx, &mut eframe::Frame::_new_kittest())
            });
            assert!(app.crash.is_none() && app.project.is_some());
            assert_eq!(matches!(app.pending, Some(Action::Quit)), dirty);
            assert_eq!(
                output.viewport_output[&egui::ViewportId::ROOT]
                    .commands
                    .iter()
                    .any(|command| matches!(command, egui::ViewportCommand::CancelClose)),
                dirty
            );
        }
    }

    #[test]
    fn command_w_closes_project_or_exits_home_when_modifiers_are_released_in_the_same_frame() {
        for (has_project, dirty) in [(true, false), (true, true), (false, false)] {
            let ctx = egui::Context::default();
            theme::configure(&ctx);
            let mut app = EditorApp::empty();
            app.repaint = Some(ctx.clone());
            if has_project {
                app.set_project(Project::new());
            }
            app.dirty = dirty;
            app_frame(&mut app, &ctx, vec![]);
            let mut input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1440., 900.),
                )),
                focused: true,
                modifiers: egui::Modifiers::NONE,
                events: [true, false]
                    .into_iter()
                    .map(|pressed| egui::Event::Key {
                        key: Key::W,
                        physical_key: Some(Key::W),
                        pressed,
                        repeat: false,
                        modifiers: egui::Modifiers {
                            command: true,
                            mac_cmd: cfg!(target_os = "macos"),
                            ctrl: !cfg!(target_os = "macos"),
                            ..Default::default()
                        },
                    })
                    .collect(),
                ..Default::default()
            };
            eframe::App::raw_input_hook(&mut app, &ctx, &mut input);
            let output = ctx.run(input, |ctx| {
                eframe::App::update(&mut app, ctx, &mut eframe::Frame::_new_kittest())
            });
            assert_eq!(app.allow_close, !has_project);
            assert_eq!(
                output.viewport_output[&egui::ViewportId::ROOT]
                    .commands
                    .iter()
                    .any(|cmd| matches!(cmd, egui::ViewportCommand::Close)),
                !has_project
            );
            if dirty {
                assert!(matches!(app.pending, Some(Action::Close)));
                assert!(app.project.is_some());
            } else {
                assert!(app.project.is_none(), "Cmd/Ctrl+W must return to Home");
            }
            assert!(app.crash.is_none());
        }
    }

    #[test]
    fn world_switcher_uses_source_labels_and_switches_without_editing() {
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let mut app = EditorApp::empty();
        app.repaint = Some(ctx.clone());
        let mut project = Project::new();
        let away = project.add_world("awayrealm").unwrap();
        project.json["worlds"][0]["identifier"] = "Northern_Harbor_02".into();
        let first = project
            .add_level(Some(away), "Away_first", 256, 256)
            .unwrap();
        let second = project
            .add_level(Some(away), "Away_second", 256, 256)
            .unwrap();
        let original = project.json.clone();
        app.set_project(project);
        app_frame(&mut app, &ctx, vec![]);
        let output = app_frame(&mut app, &ctx, vec![]);
        for (label, x) in [("NoHa02", 325.), ("awayre", 369.)] {
            assert!(
                output.shapes.iter().any(|shape| {
                    matches!(&shape.shape, egui::Shape::Text(text)
                    if text.galley.text() == label
                        && ((text.pos + text.galley.size() * 0.5)
                            .distance(egui::pos2(x, 19.))) < 1.0)
                }),
                "missing world tab {label}"
            );
        }
        app_click(&mut app, &ctx, egui::pos2(369., 19.));
        assert!(app.world);
        assert_eq!(app.active_world, Some(away));
        assert_eq!(app.selected, Some(first));

        app.navigate_level(second, false);
        app.zen = true;
        app_frame(&mut app, &ctx, vec![]);
        app_click(&mut app, &ctx, egui::pos2(369., 19.));
        assert_eq!(
            app.selected,
            Some(first),
            "Clicking the active world selects its first level"
        );
        assert!(!app.world);
        assert_eq!(app.panel, None);
        assert_eq!(app.project.as_ref().unwrap().json, original);
        assert!(!app.dirty);
    }

    #[test]
    fn revealed_zen_world_panel_covers_the_world_tabs() {
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let mut app = EditorApp::empty();
        app.repaint = Some(ctx.clone());
        let mut project = Project::new();
        let away = project.add_world("Away").unwrap();
        project.add_level(Some(away), "Remote", 256, 256).unwrap();
        app.set_project(project);
        app.zen = true;
        app_frame(&mut app, &ctx, vec![]);
        app_frame(&mut app, &ctx, vec![]);
        app_click(&mut app, &ctx, egui::pos2(369., 19.));
        assert!(app.world);

        app_frame(
            &mut app,
            &ctx,
            vec![egui::Event::PointerMoved(egui::pos2(8., 20.))],
        );
        app_frame(&mut app, &ctx, vec![]);
        let panel = egui::AreaState::load(&ctx, egui::Id::new("source-definition-panel"))
            .expect("hovering the Zen edge reveals the World panel")
            .rect();
        assert_eq!(panel.top(), 0.);
        assert_eq!(panel.bottom(), 900.);
        app_click(&mut app, &ctx, egui::pos2(325., 19.));
        assert_eq!(app.active_world, Some(away));
        assert!(app.world);

        app_click(&mut app, &ctx, egui::pos2(369., 19.));
        assert_eq!(app.active_world, Some(away));
        assert!(!app.world);
        assert!(app.panel.is_none());
    }

    #[test]
    fn world_toolbar_opens_a_floating_form_over_the_existing_layer_sidebar() {
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let mut project = Project::new();
        project
            .add_layer("Original_palette", "IntGrid", 16)
            .unwrap();
        let original = project.json.clone();
        let mut app = EditorApp::empty();
        app.repaint = Some(ctx.clone());
        app.set_project(project);
        app_frame(&mut app, &ctx, vec![]);
        app_click(&mut app, &ctx, egui::pos2(45., 18.));
        let output = app_frame(&mut app, &ctx, vec![]);
        assert_eq!(app.panel, Some(Panel::World));
        assert!(app.world);
        let panel = egui::AreaState::load(&ctx, egui::Id::new("source-definition-panel"))
            .expect("World must open the same floating panel layer as other toolbar forms")
            .rect();
        assert_eq!(panel.left(), 0.);
        assert_eq!(panel.top(), 34.);
        assert_eq!(panel.width(), 400.);
        assert!(output.shapes.iter().any(|shape| {
            matches!(&shape.shape, egui::Shape::Text(text) if text.galley.text().contains("Original_palette"))
        }), "the existing layer sidebar must remain behind the World panel");
        assert_eq!(app.project.as_ref().unwrap().json, original);
        app_click(&mut app, &ctx, egui::pos2(45., 18.));
        app_frame(&mut app, &ctx, vec![]);
        assert!(app.panel.is_none() && !app.world);
    }

    #[test]
    fn zen_world_layers_hud_stays_at_the_source_position_when_the_panel_is_hidden() {
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let mut project = Project::new();
        let far = project.add_level(None, "Behind", 256, 256).unwrap();
        project.level_mut(far).unwrap()["worldDepth"] = 2.into();
        let original = project.json.clone();
        let mut app = EditorApp::empty();
        app.set_project(project);
        app.zen = true;
        app.show_panel(Some(Panel::World));

        app_frame(&mut app, &ctx, vec![]);
        let output = app_frame(&mut app, &ctx, vec![]);
        assert!(!app.zen_sidebar.revealed);
        assert!(output.shapes.iter().any(|shape| {
            matches!(&shape.shape, egui::Shape::Text(text) if text.galley.text() == "World layers")
        }));
        let hud = || {
            egui::AreaState::load(&ctx, egui::Id::new("world-depths"))
                .unwrap()
                .rect()
        };
        let depth_one = egui::pos2(473., 110.);
        let depth_two = egui::pos2(473., 90.);
        let expected = Rect::from_min_size(egui::pos2(441., 46.), egui::vec2(109., 101.));
        assert_eq!(hud(), expected);
        app_click(&mut app, &ctx, depth_one);
        assert_eq!(app.world_depth, 1);

        app_frame(
            &mut app,
            &ctx,
            vec![egui::Event::PointerMoved(egui::pos2(8., 20.))],
        );
        app_frame(&mut app, &ctx, vec![]);
        assert!(app.zen_sidebar.revealed);
        assert_eq!(hud(), expected);
        app_click(&mut app, &ctx, depth_two);
        assert_eq!(app.world_depth, 2);
        assert!(app.world);
        assert_eq!(app.project.as_ref().unwrap().json, original);
        assert!(!app.dirty);
    }

    #[test]
    fn world_layers_hud_switches_empty_depth_without_editing_or_selecting_a_level() {
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let mut project = Project::new();
        let far = project.add_level(None, "Behind", 256, 256).unwrap();
        project.level_mut(far).unwrap()["worldDepth"] = serde_json::json!(2);
        let original = project.json.clone();
        let mut app = EditorApp::empty();
        app.set_project(project);
        let selected = app.selected;
        let title = |output: &egui::FullOutput| {
            output.shapes.iter().any(|shape| {
                matches!(&shape.shape, egui::Shape::Text(text) if text.galley.text() == "World layers")
            })
        };
        assert!(!title(&app_frame(&mut app, &ctx, vec![])));
        app_click(&mut app, &ctx, egui::pos2(45., 18.));
        let output = app_frame(&mut app, &ctx, vec![]);
        assert!(title(&output), "World mode exposes the source depth range");
        let hud = egui::AreaState::load(&ctx, egui::Id::new("world-depths"))
            .unwrap()
            .rect();
        assert_eq!(
            hud,
            Rect::from_min_size(egui::pos2(441., 46.), egui::vec2(109., 101.))
        );

        let middle = egui::pos2(473., 110.);
        assert_eq!(
            ctx.layer_id_at(middle).unwrap().id,
            egui::Id::new("world-depths")
        );
        app_click(&mut app, &ctx, middle);
        assert_eq!(app.world_depth, 1, "empty depths remain clickable");
        assert_eq!(app.selected, selected);
        assert_eq!(app.project.as_ref().unwrap().json, original);
        assert!(!app.dirty);

        let hud_id = egui::Id::new("world-depths");
        let event = |pos, button, pressed| egui::Event::PointerButton {
            pos,
            button,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        let origin = egui::pos2(900., 170.);
        app_frame(
            &mut app,
            &ctx,
            vec![
                egui::Event::Key {
                    key: Key::Space,
                    physical_key: Some(Key::Space),
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                },
                egui::Event::PointerMoved(origin),
            ],
        );
        app_frame(
            &mut app,
            &ctx,
            vec![event(origin, egui::PointerButton::Secondary, true)],
        );
        assert!(app.camera_drag.is_some(), "canvas accepts the zoom drag");
        assert_ne!(ctx.layer_id_at(middle).map(|layer| layer.id), Some(hud_id));
        app_frame(
            &mut app,
            &ctx,
            vec![event(origin, egui::PointerButton::Secondary, false)],
        );
        app_frame(&mut app, &ctx, vec![egui::Event::PointerMoved(middle)]);
        app_frame(
            &mut app,
            &ctx,
            vec![event(middle, egui::PointerButton::Primary, true)],
        );
        assert_eq!(
            ctx.layer_id_at(middle).map(|layer| layer.id),
            Some(hud_id),
            "Space+primary starting over the HUD belongs to the HUD"
        );
        app_frame(
            &mut app,
            &ctx,
            vec![event(middle, egui::PointerButton::Primary, false)],
        );

        app.project.as_mut().unwrap().level_mut(far).unwrap()["worldDepth"] =
            serde_json::json!(i32::MAX);
        let output = app_frame(&mut app, &ctx, vec![]);
        assert!(title(&output));
        assert!(
            output.shapes.len() < 1500,
            "only visible depth rows are built"
        );
        app.project.as_mut().unwrap().level_mut(far).unwrap()["worldDepth"] = serde_json::json!(0);
        assert!(!title(&app_frame(&mut app, &ctx, vec![])));
    }

    #[test]
    fn toolbar_icons_preserve_source_aspect_ratio_and_pixel_alignment() {
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let mut app = EditorApp::empty();
        app.set_project(Project::new());
        let icon_bounds = |app: &mut EditorApp| {
            let deadline = Instant::now() + std::time::Duration::from_secs(2);
            loop {
                let output = app_frame(app, &ctx, vec![]);
                let icons = output
                    .shapes
                    .iter()
                    .filter_map(|shape| match &shape.shape {
                        egui::Shape::Rect(shape) if shape.brush.is_some() => {
                            let bounds = shape.rect;
                            (bounds.top() >= 6. && bounds.bottom() <= 30. && bounds.right() < 298.)
                                .then_some(bounds)
                        }
                        _ => None,
                    })
                    .collect::<Vec<_>>();
                if icons.len() == 10 {
                    break icons;
                }
                assert!(
                    Instant::now() < deadline,
                    "toolbar icon textures did not load"
                );
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
        };
        let icons = icon_bounds(&mut app);
        assert_eq!(
            icons[0],
            Rect::from_min_max(egui::pos2(5., 9.), egui::pos2(25., 28.))
        );
        assert_eq!(icons[2].min, egui::pos2(64., 8.));
        assert_eq!(icons[7].min, egui::pos2(213., 8.));

        ctx.set_pixels_per_point(2.);
        let icons = icon_bounds(&mut app);
        assert_eq!(
            icons[0],
            Rect::from_min_max(egui::pos2(5., 8.5), egui::pos2(25., 27.))
        );
        assert_eq!(icons[2].min, egui::pos2(64.5, 8.));
        assert_eq!(icons[7].min, egui::pos2(213.5, 8.));
    }

    #[test]
    fn toolbar_fades_inactive_panel_buttons_at_full_height_and_restores_them_on_close() {
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let mut app = EditorApp::empty();
        app.set_project(Project::new());
        let layer_button = egui::pos2(105., 5.);
        let fade_color = Color32::from_rgba_premultiplied(15, 17, 20, 127);
        let fades_at = |output: &egui::FullOutput, point| {
            output.shapes.iter().any(|shape| {
                matches!(&shape.shape, egui::Shape::Rect(rect)
                if rect.fill == fade_color && rect.rect.contains(point)
                    && rect.rect.top() == 0. && rect.rect.bottom() == 36.)
            })
        };
        assert!(!fades_at(&app_frame(&mut app, &ctx, vec![]), layer_button));
        for (x, panel) in [(75., Panel::Level), (45., Panel::World)] {
            app_click(&mut app, &ctx, egui::pos2(x, 18.));
            let output = app_frame(&mut app, &ctx, vec![]);
            assert_eq!(app.panel, Some(panel));
            assert!(fades_at(&output, layer_button));
            assert!(!fades_at(&output, egui::pos2(x, 5.)));
            assert!(output.shapes.iter().any(
                |shape| matches!(&shape.shape, egui::Shape::Rect(rect)
                if rect.rect.contains(egui::pos2(105., 33.))
                    && rect.rect.top() == 30. && rect.rect.bottom() == 36.
                    && rect.fill == Color32::from_black_alpha(102))
            ));
        }
        app_click(&mut app, &ctx, egui::pos2(45., 18.));
        assert!(!fades_at(&app_frame(&mut app, &ctx, vec![]), layer_button));
        assert!(app.panel.is_none());
    }

    #[test]
    fn toolbar_buttons_toggle_and_switch_floating_panels_without_editing_the_project() {
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let mut app = EditorApp::empty();
        app.repaint = Some(ctx.clone());
        app.set_project(Project::new());
        let original = app.project.as_ref().unwrap().json.clone();
        app_frame(&mut app, &ctx, vec![]);
        for (index, panel) in [
            Panel::Project,
            Panel::World,
            Panel::Level,
            Panel::Layers,
            Panel::Entities,
            Panel::Enums,
            Panel::Tilesets,
            Panel::Help,
            Panel::Settings,
        ]
        .into_iter()
        .enumerate()
        {
            let pos = egui::pos2(index as f32 * 30. + 15., 18.);
            app_click(&mut app, &ctx, pos);
            app_frame(&mut app, &ctx, vec![]);
            assert_eq!(app.panel, Some(panel), "toolbar {panel:?}");
            assert_eq!(app.world, panel == Panel::World);
            if panel != Panel::Settings {
                let rect = egui::AreaState::load(&ctx, egui::Id::new("source-definition-panel"))
                    .unwrap()
                    .rect();
                assert_eq!(rect.min, egui::pos2(0., 34.), "{panel:?}");
                let width = panels::source_panel_width(panel);
                if matches!(panel, Panel::Project | Panel::Level) {
                    assert!(
                        (width..=width + 10.).contains(&rect.width()),
                        "{panel:?}: {rect:?}"
                    );
                } else {
                    assert_eq!(rect.width(), width, "{panel:?}");
                }
            }
            app_click(&mut app, &ctx, pos);
            app_frame(&mut app, &ctx, vec![]);
            assert!(
                app.panel.is_none() && !app.world,
                "same toolbar button closes {panel:?}"
            );
            // Leave this panel open to exercise switching on the next iteration.
            app_click(&mut app, &ctx, pos);
            app_frame(&mut app, &ctx, vec![]);
        }
        assert_eq!(app.project.as_ref().unwrap().json, original);
        assert!(!app.dirty);
    }

    #[test]
    fn floating_panel_mask_blocks_paint_and_world_panel_leaves_the_canvas_live() {
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let mut project = Project::new();
        project.add_layer("Paint", "IntGrid", 16).unwrap();
        let mut app = EditorApp::empty();
        app.repaint = Some(ctx.clone());
        app.set_project(project);
        app_frame(&mut app, &ctx, vec![]);
        let original = app.project.as_ref().unwrap().json.clone();
        app_click(&mut app, &ctx, egui::pos2(75., 18.));
        app_frame(&mut app, &ctx, vec![]);
        app_click(&mut app, &ctx, egui::pos2(1300., 500.));
        assert!(
            app.panel.is_none(),
            "clicking the modal mask closes the level panel"
        );
        assert_eq!(
            app.project.as_ref().unwrap().json,
            original,
            "masked clicks must not paint"
        );
        app_click(&mut app, &ctx, egui::pos2(45., 18.));
        app_frame(&mut app, &ctx, vec![]);
        app_click(&mut app, &ctx, egui::pos2(1300., 500.));
        assert_eq!(app.panel, Some(Panel::World));
        assert!(
            app.world,
            "WorldPanel has no modal canvas mask in the source"
        );
        assert_eq!(app.project.as_ref().unwrap().json, original);
        app_click(&mut app, &ctx, egui::pos2(361., 60.));
        assert!(
            app.panel.is_none() && !app.world,
            "closing WorldPanel exits world mode"
        );
    }

    #[test]
    fn world_shortcut_and_escape_follow_the_floating_panel_lifecycle() {
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let mut app = EditorApp::empty();
        app.repaint = Some(ctx.clone());
        app.set_project(Project::new());
        app_frame(&mut app, &ctx, vec![]);
        for (key, shift, expected) in [
            (Key::W, true, Some(Panel::World)),
            (Key::P, false, Some(Panel::Project)),
            (Key::P, false, None),
            (Key::Backtick, false, Some(Panel::World)),
            (Key::Escape, false, None),
        ] {
            for pressed in [true, false] {
                app_frame(
                    &mut app,
                    &ctx,
                    vec![egui::Event::Key {
                        key,
                        physical_key: Some(key),
                        pressed,
                        repeat: false,
                        modifiers: egui::Modifiers {
                            shift,
                            ..Default::default()
                        },
                    }],
                );
            }
            assert_eq!(app.panel, expected, "{key:?}");
            assert_eq!(app.world, expected == Some(Panel::World));
        }
    }

    #[test]
    fn cancelling_level_placement_keeps_world_panel_open() {
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let mut app = EditorApp::empty();
        app.repaint = Some(ctx.clone());
        app.set_project(Project::new());
        app_frame(&mut app, &ctx, vec![]);
        app_click(&mut app, &ctx, egui::pos2(45., 18.));
        let output = app_frame(&mut app, &ctx, vec![]);
        let pos = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text() == "CREATE A NEW LEVEL" => {
                    Some(text.pos + text.galley.size() * 0.5)
                }
                _ => None,
            })
            .unwrap();
        app_click(&mut app, &ctx, pos);
        app_frame(&mut app, &ctx, vec![]);
        assert!(app.world_context.picking);
        app_frame(
            &mut app,
            &ctx,
            vec![egui::Event::Key {
                key: Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Default::default(),
            }],
        );
        assert_eq!(app.panel, Some(Panel::World));
        assert!(
            app.world,
            "Escape cancels placement before closing WorldPanel"
        );
        assert!(!app.world_context.picking);
    }

    #[test]
    fn modal_panel_mask_blocks_the_world_context_menu() {
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let mut app = EditorApp::empty();
        app.repaint = Some(ctx.clone());
        app.set_project(Project::new());
        app_frame(&mut app, &ctx, vec![]);
        app_click(&mut app, &ctx, egui::pos2(75., 18.));
        app_frame(&mut app, &ctx, vec![]);
        assert_eq!(app.panel, Some(Panel::Level));
        for pressed in [true, false] {
            app_frame(
                &mut app,
                &ctx,
                vec![
                    egui::Event::PointerMoved(egui::pos2(1400., 880.)),
                    egui::Event::PointerButton {
                        pos: egui::pos2(1400., 880.),
                        button: egui::PointerButton::Secondary,
                        pressed,
                        modifiers: Default::default(),
                    },
                ],
            );
            assert!(app.panel.is_none());
        }
        let output = app_frame(&mut app, &ctx, vec![]);
        assert!(
            !output.shapes.iter().any(|shape| {
                matches!(&shape.shape, egui::Shape::Text(text) if text.galley.text() == "New level")
            }),
            "the dimmed canvas must not open a level-creation context menu"
        );
    }

    #[test]
    fn lone_rule_group_starts_expanded_and_its_collapse_resets_on_project_reload() {
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let mut project = Project::new();
        let layer = project.add_layer("Ground", "IntGrid", 16).unwrap();
        let atlas = project.ensure_embedded_icons().unwrap();
        project
            .update_layer_properties(layer, &serde_json::json!({"tilesetDefUid":atlas}))
            .unwrap();
        let group = project.add_auto_rule_group(layer, "Terrain").unwrap();
        project.add_auto_rule(layer, group, 1).unwrap();
        let rules = |output: &egui::FullOutput| {
            output.shapes.iter().any(|shape| {
                matches!(&shape.shape, egui::Shape::Text(text) if text.galley.text() == "1-1")
            })
        };
        let mut app = EditorApp::empty();
        app.set_project(project.clone());
        app.show_panel(Some(Panel::Rules));
        app_frame(&mut app, &ctx, vec![]);
        let output = app_frame(&mut app, &ctx, vec![]);
        assert!(rules(&output), "a loaded single group starts expanded");
        let header = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text() == "Terrain" => {
                    Some(text.pos + text.galley.size() * 0.5)
                }
                _ => None,
            })
            .expect("Terrain group header");
        app_click(&mut app, &ctx, header);
        assert!(!rules(&app_frame(&mut app, &ctx, vec![])));
        app.show_panel(None);
        app_frame(&mut app, &ctx, vec![]);
        app.show_panel(Some(Panel::Rules));
        assert!(!rules(&app_frame(&mut app, &ctx, vec![])));
        assert_eq!(app.project.as_ref().unwrap().json, project.json);
        assert!(!app.dirty);

        app.set_project(project);
        app.show_panel(Some(Panel::Rules));
        assert!(rules(&app_frame(&mut app, &ctx, vec![])));
    }

    #[test]
    fn rule_row_rejects_redundant_flips_and_enables_perlin_from_its_settings() {
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        ctx.style_mut(|style| style.animation_time = 0.);
        let mut project = Project::new();
        let layer = project.add_layer("Ground", "IntGrid", 16).unwrap();
        let atlas = project.ensure_embedded_icons().unwrap();
        project
            .update_layer_properties(layer, &serde_json::json!({"tilesetDefUid":atlas}))
            .unwrap();
        let group = project.add_auto_rule_group(layer, "Terrain").unwrap();
        let rule_id = project.add_auto_rule(layer, group, 3).unwrap();
        project
            .update_auto_rule(
                layer,
                group,
                rule_id,
                &serde_json::json!({"pattern":[0,0,0,1,0,0,0,0,0],"chance":0.0}),
            )
            .unwrap();
        let rule = |app: &EditorApp| {
            app.project.as_ref().unwrap().layer_def(layer).unwrap()["autoRuleGroups"][0]["rules"][0]
                .clone()
        };
        let mut app = EditorApp::empty();
        app.set_project(project);
        app.show_panel(Some(Panel::Rules));
        app_frame(&mut app, &ctx, vec![]);
        let output = app_frame(&mut app, &ctx, vec![]);
        let control = |label| {
            output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) if text.galley.text() == label => {
                        Some(text.pos + text.galley.size() * 0.5)
                    }
                    _ => None,
                })
                .unwrap()
        };
        assert!(
            output.shapes.iter().any(|shape| {
                matches!(&shape.shape, egui::Shape::Rect(button) if
                button.stroke.color == Color32::from_rgb(0, 157, 255))
            }),
            "the enabled stop flag shows the source blue outline"
        );
        assert_eq!(rule(&app)["chance"], 0.0);
        app_click(&mut app, &ctx, control("Y"));
        assert_eq!(rule(&app)["flipY"], false);
        assert!(!app.dirty);
        app_click(&mut app, &ctx, control("X"));
        assert_eq!(rule(&app)["flipX"], true);
        let pos = control("P");
        for pressed in [true, false] {
            app_frame(
                &mut app,
                &ctx,
                vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Secondary,
                        pressed,
                        modifiers: Default::default(),
                    },
                ],
            );
        }
        assert_eq!(rule(&app)["perlinActive"], true);
        let output = app_frame(&mut app, &ctx, vec![]);
        let random = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text().eq_ignore_ascii_case("Random") => {
                    Some(text.pos + text.galley.size() * 0.5)
                }
                _ => None,
            })
            .expect("Perlin randomize button");
        assert!(random.x < pos.x && random.y < pos.y);
        assert!(output.shapes.iter().any(|shape| {
            matches!(&shape.shape, egui::Shape::Rect(cell) if
                cell.fill == Color32::from_rgba_unmultiplied(179, 247, 0, 77))
        }));
        let seed = rule(&app)["perlinSeed"].clone();
        app_click(&mut app, &ctx, random);
        assert_ne!(rule(&app)["perlinSeed"], seed);
        assert!(
            rule(&app)["perlinSeed"]
                .as_i64()
                .is_some_and(|n| (0..99_999_999).contains(&n))
        );
    }

    #[test]
    fn wizard_managed_rules_ignore_row_input_but_keep_the_assistant_available() {
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let mut project = Project::new();
        let layer = project.add_layer("Ground", "IntGrid", 16).unwrap();
        let atlas = project.ensure_embedded_icons().unwrap();
        project
            .update_layer_properties(layer, &serde_json::json!({"tilesetDefUid":atlas}))
            .unwrap();
        let group = project.add_auto_rule_group(layer, "Terrain").unwrap();
        project.add_auto_rule(layer, group, 3).unwrap();
        project.add_auto_rule(layer, group, 3).unwrap();
        project
            .update_auto_rule_group(layer, group, &serde_json::json!({"usesWizard":true}))
            .unwrap();
        let mut app = EditorApp::empty();
        app.set_project(project.clone());
        app.open_rule_groups.insert((layer, group));
        app.show_panel(Some(Panel::Rules));
        app_frame(&mut app, &ctx, vec![]);
        let output = app_frame(&mut app, &ctx, vec![]);
        let center = |label: &str| {
            output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) if text.galley.text().eq_ignore_ascii_case(label) => {
                        Some(text.pos + text.galley.size() * 0.5)
                    }
                    _ => None,
                })
                .expect(label)
        };
        let perlin = center("P");
        app_click(&mut app, &ctx, perlin);
        assert!(!app.dirty);
        assert_eq!(app.project.as_ref().unwrap().json, project.json);
        let preview = egui::pos2(100., perlin.y - 14.);
        app_click(&mut app, &ctx, preview);
        assert!(app.rule_draft.is_none());
        assert!(app.rule_hover.is_none());
        app_click(&mut app, &ctx, center("Edit rules"));
        let wizard = app_frame(&mut app, &ctx, vec![]);
        assert!(ctx.memory(|memory| memory.top_modal_layer().is_some()));
        let cancel = wizard
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text() == "CANCEL" => {
                    Some(text.pos + text.galley.size() * 0.5)
                }
                _ => None,
            })
            .expect("Assistant cancel button");
        app_click(&mut app, &ctx, cancel);
        app_frame(&mut app, &ctx, vec![]);
        assert!(ctx.memory(|memory| memory.top_modal_layer().is_none()));
        assert_eq!(app.panel, Some(Panel::Rules));
        assert_eq!(app.project.as_ref().unwrap().json, project.json);
    }

    #[test]
    fn hovered_rule_has_direct_insert_targets_above_and_below() {
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let mut project = Project::new();
        let layer = project.add_layer("Ground", "IntGrid", 16).unwrap();
        let atlas = project.ensure_embedded_icons().unwrap();
        project
            .update_layer_properties(layer, &serde_json::json!({"tilesetDefUid":atlas}))
            .unwrap();
        let group = project.add_auto_rule_group(layer, "Terrain").unwrap();
        let original = project.add_auto_rule(layer, group, 1).unwrap();
        project
            .update_auto_rule(layer, group, original, &serde_json::json!({"pattern":[1]}))
            .unwrap();
        let neighbor = project.add_auto_rule(layer, group, 1).unwrap();
        let mut app = EditorApp::empty();
        app.set_project(project);
        app.show_panel(Some(Panel::Rules));
        app_frame(&mut app, &ctx, vec![]);
        app_frame(&mut app, &ctx, vec![]);
        app_click(&mut app, &ctx, egui::pos2(135., 241.));
        assert_eq!(app.last_rule_list_row, Some(original));
        for original_index in [1, 0] {
            let pointer = egui::Event::PointerMoved(egui::pos2(135., 241.));
            app_frame(&mut app, &ctx, vec![pointer.clone()]);
            let output = app_frame(&mut app, &ctx, vec![pointer]);
            let mut points = output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::Rect(rect)
                        if rect.rect.size() == egui::Vec2::splat(18.)
                            && rect.rect.center().x > 320.
                            && rect.rect.center().x < 350. =>
                    {
                        Some(rect.rect.center())
                    }
                    _ => None,
                })
                .collect::<Vec<_>>();
            points.sort_by(|a, b| a.y.total_cmp(&b.y));
            points.dedup();
            assert_eq!(
                points.len(),
                2,
                "the hovered row exposes both insertion positions"
            );
            let target = points[1 - original_index];
            app_frame(&mut app, &ctx, vec![egui::Event::PointerMoved(target)]);
            app_click(&mut app, &ctx, target);
            assert!(
                app.rule_draft.is_some(),
                "insertion opens the new rule editor"
            );
            let def = app.project.as_ref().unwrap().layer_def(layer).unwrap();
            let rules = def["autoRuleGroups"][0]["rules"].as_array().unwrap();
            assert_eq!(rules.len(), 3);
            assert_eq!(rules[original_index]["uid"], original);
            assert_eq!(
                rules[1 - original_index]["uid"],
                app.last_rule_list_row.unwrap()
            );
            assert_eq!(rules[2]["uid"], neighbor);
            app.finish_rule_edit();
            app_frame(&mut app, &ctx, vec![]);
        }
        app_click(&mut app, &ctx, egui::pos2(135., 241.));
        assert_eq!(app.last_rule_list_row, Some(original));
        app.show_panel(None);
        app_frame(&mut app, &ctx, vec![]);
        app.show_panel(Some(Panel::Rules));
        assert_eq!(app.last_rule_list_row, None);
    }
    #[test]
    fn remapping_a_rule_group_waits_for_the_name_confirmation() {
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let mut project = Project::new();
        let layer = project.add_layer("Ground", "IntGrid", 16).unwrap();
        let atlas = project.ensure_embedded_icons().unwrap();
        project
            .update_layer_properties(layer, &serde_json::json!({"tilesetDefUid": atlas}))
            .unwrap();
        let group = project.add_auto_rule_group(layer, "Terrain").unwrap();
        project.add_auto_rule(layer, group, 3).unwrap();
        let mut app = EditorApp::empty();
        app.set_project(project.clone());
        app.show_panel(Some(Panel::Rules));
        let button = |frame: &egui::FullOutput, label: &str| {
            frame
                .shapes
                .iter()
                .rev()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) if text.galley.text().eq_ignore_ascii_case(label) => {
                        Some(text.pos + text.galley.size() * 0.5)
                    }
                    _ => None,
                })
                .expect(label)
        };
        app_frame(&mut app, &ctx, vec![]);
        app_frame(&mut app, &ctx, vec![]);
        app_click(&mut app, &ctx, egui::pos2(333., 195.));
        let menu = app_frame(&mut app, &ctx, vec![]);
        app_click(&mut app, &ctx, button(&menu, "Duplicate and remap"));
        let remap = app_frame(&mut app, &ctx, vec![]);
        assert!(ctx.memory(|memory| memory.top_modal_layer().is_some()));
        let confirm = button(&remap, "CONFIRM");
        app_click(&mut app, &ctx, confirm);
        let name = app_frame(&mut app, &ctx, vec![]);
        button(&name, "NAME THIS NEW GROUP");
        assert!(!app.dirty);
        assert_eq!(app.project.as_ref().unwrap().json, project.json);
        app_click(&mut app, &ctx, button(&name, "CANCEL"));
        let remap = app_frame(&mut app, &ctx, vec![]);
        assert!(ctx.memory(|memory| memory.top_modal_layer().is_some()));
        app_click(&mut app, &ctx, button(&remap, "CONFIRM"));
        let name = app_frame(&mut app, &ctx, vec![]);
        app_click(&mut app, &ctx, button(&name, "VALIDATE"));
        app_frame(&mut app, &ctx, vec![]);
        assert!(app.dirty);
        let groups = app.project.as_ref().unwrap().layer_def(layer).unwrap()["autoRuleGroups"]
            .as_array()
            .unwrap();
        assert_eq!(groups.len(), 2);
        assert_eq!(
            groups[0],
            project.layer_def(layer).unwrap()["autoRuleGroups"][0]
        );
        assert_eq!(groups[1]["name"], "Terrain copy");
    }

    #[test]
    fn optional_auto_layer_rules_are_edited_in_rules_and_not_in_the_palette() {
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let mut project = Project::new();
        let layer = project.add_layer("Ground", "IntGrid", 16).unwrap();
        let atlas = project.ensure_embedded_icons().unwrap();
        project
            .update_layer_properties(layer, &serde_json::json!({"tilesetDefUid":atlas}))
            .unwrap();
        let group = project
            .add_auto_rule_group(layer, "Optional terrain")
            .unwrap();
        project
            .update_auto_rule_group(layer, group, &serde_json::json!({"isOptional":true}))
            .unwrap();
        let other = project.add_level(None, "Other", 256, 256).unwrap();
        let mut app = EditorApp::empty();
        app.set_project(project);
        let selected = app.selected.unwrap();
        app_frame(&mut app, &ctx, vec![]);
        let output = app_frame(&mut app, &ctx, vec![]);
        assert!(!output.shapes.iter().any(|shape| {
            matches!(&shape.shape, egui::Shape::Text(text)
                if matches!(text.galley.text(), "Display auto-layer tiles" | "Optional terrain"))
        }));

        app.show_panel(Some(Panel::Rules));
        app_frame(&mut app, &ctx, vec![]);
        let output = app_frame(&mut app, &ctx, vec![]);
        let label = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text() == "Optional terrain" => {
                    assert!(text.galley.job.sections[0].format.italics);
                    Some(text.pos + egui::vec2(-25.5, text.galley.size().y * 0.5))
                }
                _ => None,
            })
            .expect("group is visible in the Rules panel");
        app_click(&mut app, &ctx, label);
        let project = app.project.as_ref().unwrap();
        assert_eq!(
            project.layer(selected, 0).unwrap()["optionalRules"],
            serde_json::json!([group])
        );
        assert_eq!(
            project.layer(other, 0).unwrap()["optionalRules"],
            serde_json::json!([])
        );
        let enabled = app_frame(&mut app, &ctx, vec![]);
        assert!(enabled.shapes.iter().any(|shape| {
            matches!(&shape.shape, egui::Shape::Text(text)
                if text.galley.text() == "Optional terrain"
                    && !text.galley.job.sections[0].format.italics)
        }));
    }

    #[test]
    fn sidebar_layer_list_keeps_source_height_limits_and_scrolls_its_rows() {
        let guide_top = |output: &egui::FullOutput| {
            output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Rect(rect) if rect.fill == SIDEBAR_GUIDE_FILL => {
                        Some(rect.rect.top())
                    }
                    _ => None,
                })
                .expect("layer tool guide")
        };
        for (count, expected_guide_top) in [(1, 140.), (10, 290.)] {
            let ctx = egui::Context::default();
            theme::configure(&ctx);
            ctx.set_pixels_per_point(2.);
            let mut project = Project::new();
            for index in 0..count {
                project
                    .add_layer(&format!("Layer{index}"), "Entities", 16)
                    .unwrap();
            }
            let mut app = EditorApp::empty();
            app.set_project(project);
            let hover = egui::pos2(100., 100.);
            app_frame(&mut app, &ctx, vec![egui::Event::PointerMoved(hover)]);
            let output = app_frame(&mut app, &ctx, vec![]);
            assert_eq!(
                guide_top(&output),
                expected_guide_top,
                "{count} visible layers"
            );

            if count == 10 {
                let layer_position = |output: &egui::FullOutput| {
                    output.shapes.iter().find_map(|shape| match &shape.shape {
                        egui::Shape::Text(text) if text.galley.text() == "Layer0" => {
                            let center = text.pos + text.galley.rect.center().to_vec2();
                            shape.clip_rect.contains(center).then_some(center)
                        }
                        _ => None,
                    })
                };
                assert!(
                    layer_position(&output).is_none(),
                    "last row is initially clipped"
                );
                let center = app.viewport.center;
                let zoom = app.viewport.zoom;
                app_frame(
                    &mut app,
                    &ctx,
                    vec![egui::Event::MouseWheel {
                        unit: egui::MouseWheelUnit::Point,
                        delta: egui::vec2(0., -150.),
                        modifiers: egui::Modifiers::NONE,
                    }],
                );
                let output = (0..6)
                    .map(|_| app_frame(&mut app, &ctx, vec![]))
                    .last()
                    .unwrap();
                assert_eq!(guide_top(&output), expected_guide_top);
                assert_eq!(app.viewport.center, center);
                assert_eq!(app.viewport.zoom, zoom);
                let pos = layer_position(&output).expect("scroll exposes last layer");
                app_click(&mut app, &ctx, pos);
                assert_eq!(
                    app.layer, 9,
                    "visible layer can be selected after scrolling"
                );
            }
        }
    }

    fn generated_layer_project() -> (Project, i64, i64) {
        let mut project = Project::new();
        let source = project.add_layer("Ground", "IntGrid", 16).unwrap();
        let generated = project.add_layer("Generated", "AutoLayer", 16).unwrap();
        let atlas = project
            .add_tileset(Path::new("Atlas.png"), 32, 32, 16)
            .unwrap();
        project.assign_tileset(generated, atlas).unwrap();
        project
            .update_layer_properties(
                generated,
                &serde_json::json!({
                    "autoSourceLayerDefUid": source,
                    "autoRuleGroups": [{
                        "uid": 700, "name": "Ground", "active": true,
                        "rules": [{
                            "uid": 701, "active": true, "size": 1, "pattern": [1],
                            "tileRectsIds": [[0]], "chance": 1, "breakOnMatch": true
                        }]
                    }]
                }),
            )
            .unwrap();
        (project, generated, atlas)
    }

    #[test]
    fn automatic_layer_has_empty_palette_and_accessible_rules() {
        let (project, uid, _) = generated_layer_project();
        let mut app = EditorApp::empty();
        app.set_project(project);
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let output = app_frame(&mut app, &ctx, vec![]);

        let mut rules = None;
        for shape in &output.shapes {
            if let egui::Shape::Text(text) = &shape.shape {
                let label = text.galley.text();
                assert!(!matches!(
                    label,
                    "Generated from an IntGrid layer" | "Edit source IntGrid"
                ));
                if label == "RULES" {
                    rules = Some(text.pos + text.galley.size() * 0.5);
                }
            }
        }
        app_click(&mut app, &ctx, rules.expect("automatic layer Rules button"));
        assert_eq!(app.panel, Some(Panel::Rules));
        assert_eq!(app.definition_uid, Some(uid));
        assert!(!app.dirty);
    }

    #[test]
    fn unassigned_tile_layer_warns_until_the_level_has_a_tileset() {
        let mut project = Project::new();
        project.add_layer("Ground", "Tiles", 16).unwrap();
        let key = project.level_keys()[0];
        let mut app = EditorApp::empty();
        app.set_project(project);
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let warns = |out: &egui::FullOutput| {
            out.shapes.iter().any(|shape| {
                matches!(&shape.shape, egui::Shape::Text(text)
                    if text.galley.text() == "This tile layer has no Tileset.")
            })
        };
        assert!(warns(&app_frame(&mut app, &ctx, vec![])));
        let project = app.project.as_mut().unwrap();
        let atlas = project.ensure_embedded_icons().unwrap();
        project
            .set_layer_tileset_override(
                key,
                0,
                Some(atlas),
                ldtk_core::TilesetRemap::TileIds,
                false,
            )
            .unwrap();
        assert!(!warns(&app_frame(&mut app, &ctx, vec![])));
    }

    #[test]
    fn automatic_layer_does_not_start_an_editing_stroke() {
        let (project, uid, _) = generated_layer_project();
        let original = project.json.clone();
        let key = project.level_keys()[0];
        let mut app = EditorApp::empty();
        app.set_project(project);
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let pointer = egui::pos2(870., 450.);
        app_frame(&mut app, &ctx, vec![]);
        app_frame(&mut app, &ctx, vec![egui::Event::PointerMoved(pointer)]);
        for button in [egui::PointerButton::Primary, egui::PointerButton::Secondary] {
            let event = |pressed| egui::Event::PointerButton {
                pos: pointer,
                button,
                pressed,
                modifiers: Default::default(),
            };
            let output = app_frame(&mut app, &ctx, vec![event(true)]);
            assert!(!app.stroke);
            assert_eq!(
                output.platform_output.cursor_icon,
                egui::CursorIcon::NotAllowed
            );
            app_frame(&mut app, &ctx, vec![event(false)]);
        }
        assert_eq!(app.project.as_ref().unwrap().json, original);
        assert!(!app.dirty);

        app.switch_layer(1);
        app_click(&mut app, &ctx, pointer);
        let project = app.project.as_ref().unwrap();
        let generated = project.layer(key, 0).unwrap();
        assert_eq!(generated["layerDefUid"], uid);
        assert_eq!(generated["autoLayerTiles"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn automatic_layer_has_no_tile_selection_to_navigate_or_save() {
        let (project, _, atlas) = generated_layer_project();
        let original = project.json.clone();
        let mut app = EditorApp::empty();
        app.set_project(project);
        assert!(app.set_atlas_selection(atlas, vec![0]));
        let ctx = egui::Context::default();
        app.navigate_palette(&ctx, 1, 0);
        assert_eq!(app.selected_tile_ids_for(atlas), [0]);
        app.tile_selection_memory(&ctx, false);
        assert_eq!(app.project.as_ref().unwrap().json, original);
        assert!(!app.dirty);

        theme::configure(&ctx);
        for key in [Key::R, Key::X, Key::Y] {
            app_frame(
                &mut app,
                &ctx,
                vec![egui::Event::Key {
                    key,
                    physical_key: Some(key),
                    pressed: true,
                    repeat: false,
                    modifiers: Default::default(),
                }],
            );
        }
        assert!(!app.random_tiles);
        assert_eq!(app.flips, 0);
    }

    #[test]
    fn searchable_palettes_keep_their_toolbar_above_the_scrollable_rows() {
        let visible_text = |output: &egui::FullOutput, label: &str| {
            output.shapes.iter().find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text().ends_with(label) => {
                    let pos = text.pos + text.galley.rect.center().to_vec2();
                    shape.clip_rect.contains(pos).then_some(pos)
                }
                _ => None,
            })
        };
        for kind in ["Entities", "IntGrid"] {
            let ctx = egui::Context::default();
            theme::configure(&ctx);
            let mut project = Project::new();
            project.add_layer("Palette", kind, 16).unwrap();
            if kind == "Entities" {
                for id in 1..=40 {
                    project
                        .add_entity_def(&format!("Item{id:02}"), 16, 16)
                        .unwrap();
                }
            } else {
                project.json["defs"]["layers"][0]["intGridValues"] = Value::Array(
                    (1..=40)
                        .map(|id| {
                            serde_json::json!({"value":id, "identifier":format!("Item{id:02}"),
                            "color":"#22AA77","tile":null,"groupUid":0})
                        })
                        .collect(),
                );
            }

            let mut app = EditorApp::empty();
            app.set_project(project);
            app_frame(
                &mut app,
                &ctx,
                vec![egui::Event::PointerMoved(egui::pos2(180., 400.))],
            );
            let before = app_frame(&mut app, &ctx, vec![]);
            let search = visible_text(&before, "Search…").expect("initial search toolbar");
            let first = visible_text(&before, "Item01").expect("initial first item");
            let row_height = if kind == "Entities" { 44. } else { 46. };
            let first_row = before
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Rect(rect)
                        if rect.rect.contains(first)
                            && rect.rect.height() == row_height
                            && rect.rect.width() > 250. =>
                    {
                        Some(rect.rect)
                    }
                    _ => None,
                })
                .expect("source-sized first palette row");
            assert_eq!(first_row.min, egui::pos2(9., 227.), "{kind} row inset");
            assert!(visible_text(&before, "Item40").is_none());

            app_frame(
                &mut app,
                &ctx,
                vec![egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(0., -2400.),
                    modifiers: egui::Modifiers::NONE,
                }],
            );
            let after = (0..6)
                .map(|_| app_frame(&mut app, &ctx, vec![]))
                .last()
                .unwrap();
            assert!(
                visible_text(&after, "Item40").is_some(),
                "{kind} list must scroll to its last row"
            );
            assert_eq!(
                visible_text(&after, "Search…"),
                Some(search),
                "{kind} search should stay above scrolling items"
            );
        }
    }

    #[test]
    fn entity_and_intgrid_palette_rows_share_source_tints_and_outlines() {
        for (kind, row_height) in [("Entities", 44.), ("IntGrid", 46.)] {
            let ctx = egui::Context::default();
            theme::configure(&ctx);
            let mut project = Project::new();
            project.add_layer("Palette", kind, 16).unwrap();
            let mut app = EditorApp::empty();
            if kind == "Entities" {
                let selected = project.add_entity_def("Target", 16, 16).unwrap();
                project.add_entity_def("Other", 16, 16).unwrap();
                for entity in project.json["defs"]["entities"].as_array_mut().unwrap() {
                    entity["color"] = "#22AA77".into();
                }
                app.entity_uid = Some(selected);
            } else {
                project.json["defs"]["layers"][0]["intGridValues"] = serde_json::json!([
                    {"value":1,"identifier":"Target","color":"#22AA77","tile":null,"groupUid":0},
                    {"value":2,"identifier":null,"color":"#22AA77","tile":null,"groupUid":0}
                ]);
                app.int_value = 1;
            }
            app.set_project(project);
            app_frame(&mut app, &ctx, vec![]);
            let before = app_frame(&mut app, &ctx, vec![]);
            let has_rect = |output: &egui::FullOutput, rect, fill, stroke| {
                output.shapes.iter().any(|shape| {
                    matches!(&shape.shape, egui::Shape::Rect(shape)
                        if shape.rect == rect && shape.fill == fill && shape.stroke == stroke)
                })
            };
            let first_row = before
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Rect(shape)
                        if shape.rect.min == egui::pos2(9., 227.)
                            && shape.rect.height() == row_height
                            && shape.rect.width() > 250. =>
                    {
                        Some(shape.rect)
                    }
                    _ => None,
                })
                .expect("first palette row");
            let stripe = Rect::from_min_size(first_row.min, egui::vec2(6., row_height));
            let outline = Stroke::new(1_f32, Color32::WHITE);
            assert!(
                has_rect(
                    &before,
                    stripe,
                    Color32::from_rgb(34, 170, 119),
                    Stroke::NONE
                ),
                "{kind} row color stripe"
            );
            assert!(has_rect(&before, first_row, Color32::TRANSPARENT, outline));
            assert!(has_rect(
                &before,
                first_row,
                Color32::from_rgb(180, 226, 209),
                Stroke::NONE
            ));

            let second_row = first_row.translate(egui::vec2(0., row_height + 1.));
            let events = vec![egui::Event::PointerMoved(second_row.center())];
            app_frame(&mut app, &ctx, events);
            let hovered = app_frame(&mut app, &ctx, vec![]);
            let inactive_fill = Color32::from_rgb(45, 88, 82);
            assert!(has_rect(&before, second_row, inactive_fill, Stroke::NONE));
            assert!(!has_rect(
                &before,
                second_row,
                Color32::TRANSPARENT,
                outline
            ));
            assert!(has_rect(&hovered, second_row, inactive_fill, Stroke::NONE));
            assert!(has_rect(
                &hovered,
                second_row,
                Color32::TRANSPARENT,
                outline
            ));

            if kind == "IntGrid" {
                let labels = before.shapes.iter().filter_map(|shape| match &shape.shape {
                    egui::Shape::Text(shape) if first_row.union(second_row).contains(shape.pos) => {
                        Some(shape.galley.text())
                    }
                    _ => None,
                });
                assert_eq!(labels.collect::<Vec<_>>(), ["Target"]);
                let swatch = Rect::from_min_size(egui::pos2(25., 233.), Vec2::splat(34.));
                assert!(has_rect(
                    &before,
                    swatch,
                    Color32::from_rgb(17, 85, 60),
                    Stroke::new(1_f32, Color32::from_rgb(78, 187, 146))
                ));
            }
        }
    }

    #[test]
    fn five_layer_sidebar_matches_source_rows_guide_and_atlas_top() {
        let ctx = egui::Context::default();
        crate::theme::configure(&ctx);
        ctx.set_pixels_per_point(2.);
        let mut app = EditorApp::empty();
        app.set_project(
            Project::load(
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("examples/original/Typical_TopDown_example.ldtk"),
            )
            .unwrap(),
        );
        let project = app.project.as_ref().unwrap();
        let key = app.selected.unwrap();
        app.layer = project.level(key).unwrap()["layerInstances"]
            .as_array()
            .unwrap()
            .iter()
            .position(|l| l["__type"] == "Tiles")
            .unwrap();
        let iid = project.json["iid"].as_str().unwrap().to_owned();
        let uid = project.layer(key, app.layer).unwrap()["__tilesetDefUid"]
            .as_i64()
            .unwrap();
        let mut output = None;
        for _ in 0..3 {
            output = Some(ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1440., 900.),
                    )),
                    ..Default::default()
                },
                |ctx| app.sidebar(ctx),
            ));
        }
        let area = egui::AreaState::load(
            &ctx,
            egui::Id::new(("tile-palette-popout", &iid, uid)).with("content"),
        )
        .unwrap();
        assert_eq!(ctx.available_rect().left(), 300.);
        assert_eq!(
            area.rect().top(),
            276.,
            "source five-row sidebar starts its atlas at y276"
        );
        let shapes = output.unwrap().shapes;
        let guide = shapes
            .iter()
            .find_map(|s| match &s.shape {
                egui::Shape::Rect(r) if r.fill == SIDEBAR_GUIDE_FILL => Some(r.rect),
                _ => None,
            })
            .unwrap();
        assert_eq!(
            guide,
            Rect::from_min_size(egui::pos2(0., 200.), egui::vec2(300., 31.0625))
        );
        let rows = shapes
            .iter()
            .filter_map(|s| match &s.shape {
                egui::Shape::Rect(r) if r.rect.width() == 300. && r.rect.height() == 31. => {
                    Some(r.rect)
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(rows.len(), 5);
        for (i, row) in rows.iter().enumerate() {
            assert_eq!(row.top(), 40. + 32. * i as f32);
        }
    }
    #[test]
    fn source_sidebar_keeps_three_hundred_points_and_rules_label_visible() {
        let ctx = egui::Context::default();
        crate::theme::configure(&ctx);
        let mut app = EditorApp::empty();
        app.set_project(
            Project::load(
                PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/original/Entities.ldtk"),
            )
            .unwrap(),
        );
        let mut left = 0.;
        let mut out = None;
        for _ in 0..2 {
            out = Some(ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1440., 900.),
                    )),
                    ..Default::default()
                },
                |ctx| {
                    app.sidebar(ctx);
                    left = ctx.available_rect().left();
                },
            ));
        }
        assert_eq!(left, 300., "palette controls must not widen sidebar");
        assert!(out.unwrap().shapes.iter().any(|shape|matches!(&shape.shape,egui::Shape::Text(text)if text.galley.job.text=="RULES")),"RULES label must be painted without ellipsis or wrapping");
    }
}
