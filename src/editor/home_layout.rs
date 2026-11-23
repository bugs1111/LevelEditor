use super::*;

const HELP: Color32 = Color32::from_rgb(121, 221, 138);
const HELP_DARK: Color32 = Color32::from_rgb(60, 97, 72);
const HELP_DARKER: Color32 = Color32::from_rgb(39, 63, 47);

fn child(ui: &mut egui::Ui, rect: Rect, id: &str) -> egui::Ui {
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .id_salt(id)
            .max_rect(rect)
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    child.set_clip_rect(ui.clip_rect().intersect(rect));
    child
}

fn paint_contained(
    painter: &egui::Painter,
    texture: &egui::TextureHandle,
    rect: Rect,
    align: egui::Align2,
) {
    let size = texture.size_vec2();
    let scale = (rect.width() / size.x).min(rect.height() / size.y);
    painter.image(
        texture.id(),
        align.align_size_within_rect(size * scale, rect),
        Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1., 1.)),
        Color32::WHITE,
    );
}

fn button(
    ui: &mut egui::Ui,
    width: f32,
    image: egui::ImageSource<'static>,
    label: &str,
    color: Color32,
) -> egui::Response {
    let show_chevron = label == "Examples";
    let label = locale::tr(ui, label);
    let font = FontId::new(
        if ui.ctx().content_rect().width() <= 1000. {
            12.
        } else {
            14.667
        },
        egui::FontFamily::Proportional,
    );
    let response = panels::source_controls::add_sized_button(
        ui,
        [width, 38.],
        egui::Button::new("")
            .fill(color)
            .stroke(Stroke::NONE)
            .corner_radius(2),
    );
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, response.enabled(), label)
    });
    let galley = ui.fonts_mut(|fonts| fonts.layout_no_wrap(label.into(), font, Color32::WHITE));
    let label_width = if label.is_empty() {
        0.
    } else {
        6. + galley.size().x
    };
    let group_width = 25. + label_width + if show_chevron { 22. } else { 0. };
    let left = response.rect.center().x - group_width * 0.5;
    egui::Image::new(image).paint_at(
        ui,
        Rect::from_center_size(
            egui::pos2(left + 12.5, response.rect.center().y),
            Vec2::splat(25.),
        ),
    );
    if !label.is_empty() {
        ui.painter().galley(
            egui::pos2(left + 31., response.rect.center().y - galley.size().y * 0.5),
            galley,
            Color32::WHITE,
        );
    }
    if show_chevron {
        egui::Image::new(egui::include_image!("../../assets/icons/expanded.svg")).paint_at(
            ui,
            Rect::from_center_size(
                egui::pos2(left + group_width - 8., response.rect.center().y),
                egui::vec2(16., 16.),
            ),
        );
    }
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn left_button(
    ui: &mut egui::Ui,
    width: f32,
    height: f32,
    image: egui::ImageSource<'static>,
    label: &str,
    color: Color32,
) -> egui::Response {
    let label = locale::tr(ui, label);
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, height), Sense::click());
    let fill = if response.hovered() {
        if color == theme::MEDIUM {
            theme::LIGHT
        } else {
            Color32::from_rgb(52, 143, 213)
        }
    } else {
        color
    };
    ui.painter().rect_filled(rect, 0., fill);
    egui::Image::new(image).paint_at(
        ui,
        Rect::from_center_size(rect.left_center() + egui::vec2(20., 0.), Vec2::splat(25.)),
    );
    ui.painter().text(
        rect.left_center() + egui::vec2(39., 0.),
        egui::Align2::LEFT_CENTER,
        label,
        FontId::proportional(14.667),
        Color32::WHITE,
    );
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn matches_search(text: &str, query: &str) -> bool {
    let text = text.to_lowercase();
    query
        .split_whitespace()
        .all(|word| text.contains(&word.to_lowercase()))
}

fn path_job(
    path: &std::path::Path,
    color: Color32,
    folder: bool,
    missing: bool,
) -> egui::text::LayoutJob {
    let path = path.to_string_lossy().replace('\\', "/");
    let parts = path.split('/').collect::<Vec<_>>();
    let mut job = egui::text::LayoutJob::default();
    for (index, part) in parts.iter().enumerate() {
        if index > 0 {
            job.append(
                "/",
                2.,
                egui::TextFormat {
                    font_id: FontId::new(16., egui::FontFamily::Name("body-bold".into())),
                    color: if missing {
                        color
                    } else {
                        Color32::from_rgb(115, 127, 157)
                    },
                    strikethrough: if missing {
                        Stroke::new(1_f32, color)
                    } else {
                        Stroke::NONE
                    },
                    ..Default::default()
                },
            );
        }
        let last = folder && index == parts.len() - 1;
        let color = if last && !missing {
            Color32::WHITE
        } else {
            color
        };
        job.append(
            part,
            if index > 0 { 2. } else { 0. },
            egui::TextFormat {
                font_id: if last {
                    FontId::new(13.333, egui::FontFamily::Name("body-bold".into()))
                } else {
                    FontId::proportional(12.)
                },
                color,
                strikethrough: if missing {
                    Stroke::new(1_f32, color)
                } else {
                    Stroke::NONE
                },
                ..Default::default()
            },
        );
    }
    if !folder {
        job.append(
            "/",
            2.,
            egui::TextFormat {
                font_id: FontId::new(16., egui::FontFamily::Name("body-bold".into())),
                color: Color32::from_rgb(115, 127, 157),
                ..Default::default()
            },
        );
    }
    job.wrap.max_rows = 1;
    job.wrap.break_anywhere = true;
    job
}

fn scrollbar(ui: &mut egui::Ui, width: f32) {
    ui.spacing_mut().scroll = egui::style::ScrollStyle::solid();
    ui.spacing_mut().scroll.bar_width = width;
    ui.spacing_mut().scroll.bar_inner_margin = 0.;
    ui.spacing_mut().scroll.bar_outer_margin = 0.;
    ui.visuals_mut().extreme_bg_color = theme::MEDIUM;
    ui.visuals_mut().widgets.inactive.bg_fill = Color32::from_rgb(115, 127, 157);
    ui.visuals_mut().widgets.hovered.bg_fill = theme::TEXT;
    ui.visuals_mut().widgets.active.bg_fill = theme::TEXT;
    ui.visuals_mut().widgets.inactive.corner_radius = egui::CornerRadius::ZERO;
    ui.visuals_mut().widgets.hovered.corner_radius = egui::CornerRadius::ZERO;
    ui.visuals_mut().widgets.active.corner_radius = egui::CornerRadius::ZERO;
}

impl EditorApp {
    pub(in crate::app) fn library_home(
        &mut self,
        ctx: &egui::Context,
        parent: &mut egui::Ui,
        rect: Rect,
    ) {
        let width = (rect.width() * 0.8).clamp(960., 1200.);
        let wrapper = Rect::from_center_size(rect.center(), egui::vec2(width, rect.height()));
        let header = Rect::from_min_max(
            wrapper.min,
            egui::pos2(wrapper.right(), wrapper.top() + rect.height() * 0.32),
        );
        let main = Rect::from_min_max(
            egui::pos2(wrapper.left(), header.bottom()),
            egui::pos2(wrapper.right(), wrapper.top() + rect.height() * 0.9),
        );
        let footer = Rect::from_min_max(egui::pos2(wrapper.left(), main.bottom()), wrapper.max);
        if let Some(texture) = self.assets.builtin(
            ctx,
            "home-header",
            include_bytes!("../../assets/img/homeBg_header.png"),
        ) {
            paint_contained(
                parent.painter(),
                &texture,
                header,
                egui::Align2::CENTER_BOTTOM,
            );
        }
        if let Some(texture) = self.assets.builtin(
            ctx,
            "home-main",
            include_bytes!("../../assets/img/homeBg_main.png"),
        ) {
            paint_contained(parent.painter(), &texture, main, egui::Align2::CENTER_TOP);
        }
        if let Some(texture) =
            self.assets
                .builtin(ctx, "home-icon", include_bytes!("../../assets/appIcon.png"))
        {
            paint_contained(
                parent.painter(),
                &texture,
                Rect::from_center_size(
                    egui::pos2(header.left() + 70., header.center().y),
                    Vec2::splat(140.),
                ),
                egui::Align2::CENTER_CENTER,
            );
        }
        if let Some(texture) = self.assets.builtin(
            ctx,
            "home-logo",
            include_bytes!("../../assets/img/LDtk-shine-1024.png"),
        ) {
            paint_contained(
                parent.painter(),
                &texture,
                Rect::from_center_size(
                    egui::pos2(header.left() + 306., header.center().y),
                    egui::vec2(300., 128.),
                ),
                egui::Align2::LEFT_CENTER,
            );
        }
        let version = env!("CARGO_PKG_VERSION")
            .strip_suffix(".0")
            .unwrap_or(env!("CARGO_PKG_VERSION"));
        let mut job = egui::text::LayoutJob::default();
        job.append(
            version,
            0.,
            egui::TextFormat {
                font_id: FontId::new(140., egui::FontFamily::Name("heading".into())),
                color: theme::LIGHT,
                extra_letter_spacing: -8.4,
                ..Default::default()
            },
        );
        let galley = parent.fonts_mut(|fonts| fonts.layout_job(job));
        let version_rect = Rect::from_min_size(
            egui::pos2(
                header.right() - 16. - galley.size().x,
                header.center().y - galley.size().y * 0.5 + 4.,
            ),
            galley.size(),
        );
        parent
            .painter()
            .galley(version_rect.min, galley, theme::LIGHT);
        let mut action = None;
        let recent = self.settings.value["recentProjects"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let directories = self.settings.value["recentDirs"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        home_files::prepare(
            ctx,
            recent
                .iter()
                .filter_map(Value::as_str)
                .map(PathBuf::from)
                .collect(),
        );
        let files = Rect::from_min_max(
            main.min + egui::vec2(0., 30.),
            egui::pos2(main.right() - 250., main.bottom()),
        );
        let about = Rect::from_min_max(egui::pos2(files.right() + 16., files.top()), main.max);
        let mut ui = child(parent, files, "home-files");
        ui.spacing_mut().item_spacing = egui::vec2(2., 8.);
        let mut hide = self.settings.ui_state_int("HideSamplesOnHome", None, 0) != 0;
        let initially_hidden = hide;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x=2.;
            ui.spacing_mut().button_padding=egui::vec2(8.,0.);
            let fullscreen=ctx.input(|input| input.viewport().fullscreen.unwrap_or(false));
            let import_font = FontId::proportional(if ctx.content_rect().width() <= 1000. {12.} else {14.667});
            let import_label = locale::tr(ui, "Import");
            let import_width = 47. + ui.fonts_mut(|fonts| fonts.layout_no_wrap(import_label.into(), import_font, Color32::WHITE).size().x);
            let flexible=(ui.available_width()-import_width-41.-if fullscreen {41.+2.} else {0.}-8.)/3.;
            if button(ui,flexible,egui::include_image!("../../assets/icons/tutorial.svg"),"Examples",Color32::from_rgb(73,141,85)).on_hover_text(locale::tr(ui, "These examples showcase various functionalities from LDtk editor, including auto-layer rules and entities.")).clicked() { hide = !hide; }
            if button(ui,flexible,egui::include_image!("../../assets/icons/new.svg"),"New",Color32::from_rgb(227,77,0)).clicked() { action=Some(Action::New); }
            if button(ui,flexible,egui::include_image!("../../assets/icons/open.svg"),"Load",Color32::from_rgb(227,77,0)).clicked() { action=Some(Action::Browse); }
            let import=button(ui,import_width,egui::include_image!("../../assets/icons/open.svg"),"Import",theme::LIGHT);
            egui::Popup::menu(&import).show(|ui| {
                ui.weak(locale::tr(ui, "Import a project from another app"));
                if ui.button(locale::tr(ui, "Ogmo 3 project")).clicked() { action=Some(Action::BrowseOgmo);ui.close(); }
            });
            if button(ui,41.,egui::include_image!("../../assets/icons/settings.svg"),"",theme::LIGHT).on_hover_text(locale::tr(ui, "Application settings")).clicked() { self.show_panel(Some(Panel::Settings)); }
            if fullscreen && button(ui,41.,egui::include_image!("../../assets/icons/exit.svg"),"",theme::DARK).on_hover_text(locale::tr(ui, "Close app")).clicked() { action=Some(Action::Quit); }
        });
        if !hide {
            self.home_samples(ctx, &mut ui, &mut hide, &mut action);
        }
        if hide != initially_hidden {
            self.settings
                .set_ui_state_int("HideSamplesOnHome", None, i64::from(hide));
        }
        let search_id = egui::Id::new("source-home-search");
        let search_input = search_id.with("input");
        let mut search = ctx
            .data_mut(|data| data.get_temp::<String>(search_id))
            .unwrap_or_default();
        let search_was_focused = ctx.memory(|mem| {
            mem.top_modal_layer().is_none()
                && (mem.has_focus(search_input) || mem.had_focus_last_frame(search_input))
        });
        if search_was_focused
            && ctx.input_mut(|input| {
                input.focused && input.consume_key(egui::Modifiers::NONE, egui::Key::Escape)
            })
        {
            search.clear();
            ctx.memory_mut(|mem| mem.request_focus(search_input));
        }
        let search_glow = ui.painter().add(egui::Shape::Noop);
        let search_frame = ui.painter().add(egui::Shape::Noop);
        let cursor_stroke = ui.visuals().text_cursor.stroke;
        ui.visuals_mut().text_cursor.stroke = Stroke::new(1_f32, Color32::WHITE);
        let search_response = ui.add_sized(
            [ui.available_width(), 33.],
            egui::TextEdit::singleline(&mut search)
                .id(search_input)
                .font(FontId::proportional(14.667))
                .text_color(Color32::WHITE)
                .hint_text(
                    RichText::new(locale::tr(&ui, "Search..."))
                        .color(Color32::from_rgb(117, 117, 117)),
                )
                .frame(false)
                .margin(egui::Margin {
                    left: 29,
                    right: 36,
                    top: 8,
                    bottom: 8,
                }),
        );
        ui.visuals_mut().text_cursor.stroke = cursor_stroke;
        if search_response.has_focus() {
            ui.painter().set(
                search_glow,
                egui::epaint::Shadow {
                    offset: [0, 0],
                    blur: 4,
                    spread: 0,
                    color: Color32::from_rgb(227, 77, 0),
                }
                .as_shape(search_response.rect, 16.),
            );
        }
        ui.painter().set(
            search_frame,
            egui::epaint::RectShape::new(
                search_response.rect,
                16,
                theme::DARK,
                Stroke::new(
                    1_f32,
                    if search_response.has_focus() {
                        theme::ACCENT
                    } else {
                        theme::LIGHT
                    },
                ),
                egui::StrokeKind::Inside,
            ),
        );
        egui::Image::new(egui::include_image!("../../assets/icons/search.svg")).paint_at(
            &ui,
            Rect::from_center_size(
                search_response.rect.left_center() + egui::vec2(17., 0.),
                Vec2::splat(16.),
            ),
        );
        if !search.is_empty() {
            let icon_size = 19.;
            let rect = Rect::from_center_size(
                search_response.rect.right_center() - egui::vec2(12. + icon_size * 0.5, 0.),
                egui::vec2(icon_size, search_response.rect.height()),
            );
            let clear = ui
                .interact(rect, search_id.with("clear"), Sense::click())
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .on_hover_text(locale::tr(&ui, "Clear"));
            if clear.clicked() {
                search.clear();
                ctx.request_repaint();
            } else {
                egui::Image::new(egui::include_image!("../../assets/icons/clear.svg"))
                    .tint(Color32::WHITE.gamma_multiply(if clear.hovered() { 1. } else { 0.5 }))
                    .paint_at(
                        &ui,
                        Rect::from_center_size(rect.center(), Vec2::splat(icon_size)),
                    );
            }
        }
        ctx.data_mut(|data| data.insert_temp(search_id, search.clone()));
        let list_rect = Rect::from_min_max(ui.cursor().min, files.max);
        let column_width = (list_rect.width() - 8.) * 0.5;
        for (index, entries) in [&recent, &directories].into_iter().enumerate() {
            let rect = Rect::from_min_size(
                list_rect.min + egui::vec2(index as f32 * (column_width + 8.), 0.),
                egui::vec2(column_width, list_rect.height()),
            );
            self.home_recent_list(
                &mut child(
                    &mut ui,
                    rect,
                    if index == 0 {
                        "recent-projects"
                    } else {
                        "recent-folders"
                    },
                ),
                entries,
                index == 0,
                &search,
                &mut action,
            );
        }
        self.home_about(&mut child(parent, about, "home-about"));
        if let Some(texture) = self.assets.builtin(
            ctx,
            "home-deepnight",
            include_bytes!("../../assets/img/deepnight.png"),
        ) {
            let logo = Rect::from_min_size(
                egui::pos2(footer.left(), footer.center().y - 20.),
                egui::vec2(111., 40.),
            );
            paint_contained(parent.painter(), &texture, logo, egui::Align2::LEFT_CENTER);
            if parent
                .interact(logo, parent.id().with("deepnight-link"), Sense::click())
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .clicked()
            {
                ctx.open_url(egui::OpenUrl::new_tab("https://deepnight.net"));
            }
        }
        parent.painter().text(
            egui::pos2(footer.right(), footer.center().y + 9.),
            egui::Align2::RIGHT_CENTER,
            locale::tr(
                parent,
                "If you read this, you're awesome! Have a nice day <3",
            ),
            FontId::proportional(12.),
            theme::LIGHT,
        );
        self.home_support_dialog(ctx);
        self.home_backup_delete_dialog(ctx);
        self.recent_directory_color_dialog(ctx);
        if let Some(action) = action {
            self.request(ctx, action);
        }
    }

    fn home_samples(
        &mut self,
        ctx: &egui::Context,
        ui: &mut egui::Ui,
        hide: &mut bool,
        action: &mut Option<Action>,
    ) {
        egui::Frame::new()
            .fill(HELP_DARKER)
            .stroke(Stroke::new(1_f32, HELP))
            .corner_radius(3)
            .inner_margin(4)
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 4.;
                ui.horizontal(|ui| {
                    let (title, _) = ui.allocate_exact_size(
                        egui::vec2(ui.available_width() - 29., 25.),
                        Sense::hover(),
                    );
                    ui.painter().text(
                        title.left_center() + egui::vec2(4., 0.),
                        egui::Align2::LEFT_CENTER,
                        locale::tr(ui, "Example projects to discover LDtk"),
                        FontId::proportional(13.333),
                        HELP,
                    );
                    let close = ui
                        .scope(|ui| {
                            ui.spacing_mut().button_padding = Vec2::ZERO;
                            ui.add_sized(
                                [27., 25.],
                                egui::Button::image(
                                    egui::Image::new(egui::include_image!(
                                        "../../assets/icons/close.png"
                                    ))
                                    .fit_to_exact_size(Vec2::splat(23.)),
                                )
                                .fill(HELP_DARK)
                                .corner_radius(0)
                                .stroke(Stroke::new(1_f32, Color32::from_rgb(94, 171, 107))),
                            )
                        })
                        .inner;
                    if close.hovered() {
                        ui.painter().rect_stroke(
                            close.rect,
                            0.,
                            Stroke::new(1_f32, Color32::WHITE),
                            egui::StrokeKind::Inside,
                        );
                    }
                    if close
                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                        .clicked()
                    {
                        *hide = true;
                    }
                });
                egui::Frame::new()
                    .fill(HELP_DARK)
                    .inner_margin(egui::Margin {
                        left: 4,
                        right: 4,
                        top: 4,
                        bottom: 8,
                    })
                    .show(ui, |ui| {
                        scrollbar(ui, 15.);
                        egui::ScrollArea::horizontal()
                            .id_salt("source-home-samples")
                            .auto_shrink([false, true])
                            .max_height(148.)
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing.x = 4.;
                                    let samples = ctx.data_mut(|data| {
                                        data.get_temp_mut_or_insert_with(
                                            egui::Id::new("source-home-sample-paths"),
                                            || project_files(&resource_path("examples/original")),
                                        )
                                        .clone()
                                    });
                                    for path in samples {
                                        let (rect, response) = ui.allocate_exact_size(
                                            egui::vec2(128., 131.),
                                            Sense::click(),
                                        );
                                        let thumb =
                                            Rect::from_min_size(rect.min, egui::vec2(128., 96.));
                                        let name =
                                            path.file_stem().unwrap_or_default().to_string_lossy();
                                        let seen = self.settings.value["recentProjects"]
                                            .as_array()
                                            .is_some_and(|entries| {
                                                entries
                                                    .iter()
                                                    .any(|entry| entry.as_str() == path.to_str())
                                            });
                                        let tint = if seen {
                                            Color32::from_white_alpha(153)
                                        } else {
                                            Color32::WHITE
                                        };
                                        ui.painter().rect_filled(
                                            rect,
                                            4.,
                                            if seen {
                                                Color32::from_black_alpha(153)
                                            } else {
                                                Color32::BLACK
                                            },
                                        );
                                        if let Some(texture) = self.assets.texture(
                                            ctx,
                                            &path
                                                .with_file_name("thumbs")
                                                .join(format!("{name}.png")),
                                        ) {
                                            let aspect =
                                                texture.size_vec2().x / texture.size_vec2().y;
                                            let ratio = thumb.width() / thumb.height();
                                            let uv = if aspect > ratio {
                                                let w = ratio / aspect;
                                                Rect::from_min_max(
                                                    egui::pos2((1. - w) * 0.5, 0.),
                                                    egui::pos2((1. + w) * 0.5, 1.),
                                                )
                                            } else {
                                                let h = aspect / ratio;
                                                Rect::from_min_max(
                                                    egui::pos2(0., (1. - h) * 0.5),
                                                    egui::pos2(1., (1. + h) * 0.5),
                                                )
                                            };
                                            egui::Image::from_texture(&texture)
                                                .uv(uv)
                                                .tint(tint)
                                                .corner_radius(egui::CornerRadius {
                                                    nw: 4,
                                                    ne: 4,
                                                    sw: 0,
                                                    se: 0,
                                                })
                                                .paint_at(ui, thumb);
                                        }
                                        for inset in 0..8 {
                                            ui.painter().rect_stroke(
                                                thumb.shrink(inset as f32),
                                                3.,
                                                Stroke::new(
                                                    1_f32,
                                                    Color32::from_black_alpha(
                                                        ((1. - inset as f32 / 8.).powi(2) * 90.)
                                                            as u8,
                                                    ),
                                                ),
                                                egui::StrokeKind::Inside,
                                            );
                                        }
                                        let text_rect = Rect::from_min_max(
                                            thumb.left_bottom() + egui::vec2(2., 4.),
                                            rect.right_bottom() - egui::vec2(2., 4.),
                                        );
                                        let galley = ui.painter().layout(
                                            name.replace('_', " "),
                                            FontId::proportional(11.333),
                                            if seen {
                                                theme::TEXT.gamma_multiply(0.6)
                                            } else {
                                                tint
                                            },
                                            text_rect.width(),
                                        );
                                        ui.painter().galley(
                                            text_rect.center() - galley.size() * 0.5,
                                            galley,
                                            tint,
                                        );
                                        if response.hovered() {
                                            ui.painter().rect_stroke(
                                                rect,
                                                4.,
                                                Stroke::new(1_f32, Color32::WHITE),
                                                egui::StrokeKind::Inside,
                                            );
                                        }
                                        if response
                                            .on_hover_cursor(egui::CursorIcon::PointingHand)
                                            .clicked()
                                        {
                                            *action = Some(Action::Open(path));
                                        }
                                    }
                                });
                            });
                    });
            });
    }

    fn home_recent_list(
        &mut self,
        ui: &mut egui::Ui,
        entries: &[Value],
        files: bool,
        query: &str,
        action: &mut Option<Action>,
    ) {
        let mut paths = entries.iter().filter_map(Value::as_str).peekable();
        let has_history = paths.peek().is_some();
        let entries: Vec<PathBuf> = paths
            .filter(|path| matches_search(path, query))
            .map(PathBuf::from)
            .collect();
        let rect = ui.max_rect();
        ui.painter().rect_filled(rect, 0., theme::DARK);
        ui.painter().rect_stroke(
            rect,
            0.,
            Stroke::new(1_f32, theme::LIGHT),
            egui::StrokeKind::Inside,
        );
        if !has_history {
            egui::Image::new(egui::include_image!("../../assets/img/stripes.png"))
                .texture_options(egui::TextureOptions::NEAREST_REPEAT)
                .uv(Rect::from_min_max(
                    egui::Pos2::ZERO,
                    egui::pos2(rect.width() / 8., rect.height() / 8.),
                ))
                .tint(Color32::from_white_alpha(100))
                .paint_at(ui, rect);
            ui.painter().text(
                egui::pos2(rect.center().x, rect.top() + 49.),
                egui::Align2::CENTER_CENTER,
                if files {
                    locale::tr(ui, "No recent project file")
                } else {
                    locale::tr(ui, "No recent folder")
                },
                FontId::new(20.8, egui::FontFamily::Name("panel-title".into())),
                Color32::from_rgb(115, 127, 157),
            );
            return;
        }
        let title = if files {
            locale::tr(ui, "Recent projects")
        } else {
            locale::tr(ui, "Recent folders")
        };
        let show_title = matches_search(title, query);
        if entries.is_empty() && !show_title {
            return;
        }
        let mut content = child(ui, rect.shrink(4.), "source-recent-inner");
        scrollbar(&mut content, 10.);
        egui::ScrollArea::vertical()
            .id_salt(if files { "files" } else { "folders" })
            .auto_shrink([false, false])
            .max_height(rect.height() - 8.)
            .show(&mut content, |ui| {
                ui.spacing_mut().item_spacing.y = 0.;
                if show_title {
                    ui.add_space(3.);
                    ui.label(
                        RichText::new(title)
                            .font(FontId::new(
                                11.733,
                                egui::FontFamily::Name("panel-title".into()),
                            ))
                            .color(theme::ACCENT),
                    );
                    ui.add_space(3.);
                }
                ui.spacing_mut().item_spacing.y = 1.;
                for path in entries {
                    let directory = if files {
                        path.parent().unwrap_or(std::path::Path::new("."))
                    } else {
                        &path
                    };
                    let color = self.settings.dir_color(directory);
                    let color =
                        Color32::from_rgb((color >> 16) as u8, (color >> 8) as u8, color as u8);
                    let forced = self.settings.value["recentDirColors"]
                        .as_array()
                        .is_some_and(|colors| {
                            colors
                                .iter()
                                .any(|entry| entry["path"].as_str() == directory.to_str())
                        });
                    let sample = path.starts_with(resource_path("examples/original"));
                    let (row, response) = ui.allocate_exact_size(
                        egui::vec2(
                            ui.available_width(),
                            if files && !sample {
                                50.
                            } else if files {
                                37.
                            } else {
                                31.
                            },
                        ),
                        Sense::click(),
                    );
                    ui.painter().rect_filled(
                        row,
                        0.,
                        if forced || !files {
                            mix_color(theme::DARK, color, if forced { 0.4 } else { 0.1 })
                        } else {
                            theme::MEDIUM
                        },
                    );
                    if response.hovered() {
                        ui.painter().rect_stroke(
                            row,
                            0.,
                            Stroke::new(2_f32, Color32::from_white_alpha(166)),
                            egui::StrokeKind::Inside,
                        );
                    }
                    let missing = !path.exists();
                    let text_color = if missing {
                        Color32::from_rgb(255, 163, 134)
                    } else if sample {
                        HELP
                    } else {
                        mix_color(color, Color32::WHITE, if files { 0.4 } else { 0.6 })
                    };
                    let title = if files {
                        path.file_stem().unwrap_or_default().to_string_lossy()
                    } else {
                        path.to_string_lossy()
                    };
                    let padding_y = if files { 6. } else { 2. };
                    let menu_rect = Rect::from_min_max(
                        egui::pos2(row.right() - 26., row.top() + padding_y),
                        egui::pos2(row.right() - 8., row.bottom() - padding_y),
                    );
                    let menu_response = ui
                        .interact(
                            menu_rect,
                            ui.id().with(("recent-context", &path)),
                            Sense::click(),
                        )
                        .on_hover_cursor(egui::CursorIcon::PointingHand);
                    ui.painter().rect(
                        menu_rect,
                        2.,
                        Color32::from_black_alpha(if menu_response.hovered() { 128 } else { 26 }),
                        Stroke::new(1_f32, Color32::from_black_alpha(51)),
                        egui::StrokeKind::Inside,
                    );
                    egui::Image::new(egui::include_image!("../../assets/icons/contextMenu.svg"))
                        .paint_at(
                            ui,
                            Rect::from_center_size(menu_rect.center(), egui::vec2(14.667, 24.933)),
                        );
                    let menu_frame = panels::source_controls::context_menu_frame(ui.style());
                    egui::Popup::menu(&menu_response)
                        .frame(menu_frame)
                        .show(|ui| self.recent_path_menu(ui, &path, files));
                    let mut text_right = menu_rect.left() - 8.;
                    if files {
                        let mut backup = child(
                            ui,
                            Rect::from_min_size(
                                egui::pos2(row.right() - 63., row.top() + padding_y),
                                egui::vec2(29., row.height() - 2. * padding_y),
                            ),
                            &format!("recent-backups-{}", path.display()),
                        );
                        self.home_backups_menu(&mut backup, &path);
                        if backup.min_size().x > 0. {
                            text_right = backup.max_rect().left() - 8.;
                        }
                    }
                    let text_rect = Rect::from_min_max(
                        row.min
                            + egui::vec2(
                                8.,
                                if files && !sample {
                                    6.
                                } else {
                                    (row.height() - 19.) / 2.
                                },
                            ),
                        egui::pos2(text_right, row.bottom()),
                    );
                    let font = FontId::new(
                        16.,
                        if missing {
                            egui::FontFamily::Proportional
                        } else {
                            egui::FontFamily::Name("body-bold".into())
                        },
                    );
                    let mut job = if files {
                        egui::text::LayoutJob::simple(
                            title.to_string(),
                            font,
                            text_color,
                            text_rect.width(),
                        )
                    } else {
                        path_job(&path, text_color, true, missing)
                    };
                    if sample && !files {
                        for section in &mut job.sections {
                            section.format.color = text_color;
                        }
                    }
                    job.wrap.max_width = text_rect.width();
                    job.wrap.max_rows = 1;
                    job.wrap.break_anywhere = true;
                    if missing && files {
                        job.sections[0].format.strikethrough = Stroke::new(1_f32, text_color);
                    }
                    let galley = ui.fonts_mut(|fonts| fonts.layout_job(job));
                    ui.painter().galley(text_rect.min, galley, text_color);
                    if files && !sample {
                        let path_color = mix_color(color, Color32::WHITE, 0.6);
                        let mut job = path_job(directory, path_color, false, false);
                        for section in &mut job.sections {
                            section.format.color = section.format.color.gamma_multiply(0.55);
                        }
                        job.wrap.max_width = text_rect.width();
                        job.wrap.max_rows = 1;
                        job.wrap.break_anywhere = true;
                        let galley = ui.fonts_mut(|fonts| fonts.layout_job(job));
                        ui.painter().galley(
                            egui::pos2(text_rect.left(), row.top() + 25.),
                            galley,
                            path_color,
                        );
                    }
                    if response
                        .clone()
                        .on_hover_text(path.to_string_lossy())
                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                        .clicked()
                    {
                        if files {
                            *action = Some(Action::Open(path.clone()));
                        } else if !missing {
                            *action = Some(Action::BrowseInDir(path.clone()));
                        } else {
                            if let Some(entries) = self.settings.value["recentDirs"].as_array_mut()
                            {
                                entries.retain(|entry| entry.as_str() != path.to_str());
                            }
                            let _ = self.settings.save();
                            if let Some(parent) = path.parent().filter(|parent| parent.is_dir()) {
                                *action = Some(Action::BrowseInDir(parent.to_owned()));
                            } else {
                                self.status = "Removed lost folder from history".into();
                                self.status_error = true;
                            }
                        }
                    }
                    egui::Popup::context_menu(&response)
                        .frame(menu_frame)
                        .show(|ui| self.recent_path_menu(ui, &path, files));
                }
            });
    }

    fn home_about(&mut self, ui: &mut egui::Ui) {
        ui.painter().rect_filled(ui.max_rect(), 4., theme::MEDIUM);
        ui.spacing_mut().item_spacing.y = 2.;
        let width = ui.available_width();
        let support = left_button(
            ui,
            width,
            40.,
            egui::include_image!("../../assets/icons/love.svg"),
            "Support my work",
            Color32::from_rgb(42, 115, 171),
        );
        panels::source_controls::decorate(ui, &support);
        if support.clicked() {
            ui.ctx()
                .data_mut(|data| data.insert_temp(egui::Id::new("home-support-open"), true));
        }
        let (version, title) = help::latest_changelog_heading();
        let description = if title.is_empty() {
            locale::tr(ui, "Release notes").to_owned()
        } else {
            format!("\"{title}\"")
        };
        let (rect, response) = ui.allocate_exact_size(egui::vec2(width, 96.), Sense::click());
        ui.painter().rect_filled(rect, 0., theme::LIGHT);
        ui.painter().text(
            rect.min + egui::vec2(8., 17.),
            egui::Align2::LEFT_TOP,
            locale::format(ui, "Update {version}", &[("version", version)]),
            FontId::proportional(14.667),
            theme::TEXT,
        );
        egui::Image::new(egui::include_image!("../../assets/icons/update.svg"))
            .tint(theme::ACCENT)
            .paint_at(
                ui,
                Rect::from_min_size(rect.min + egui::vec2(8., 43.), Vec2::splat(32.)),
            );
        ui.painter().text(
            rect.min + egui::vec2(48., 48.),
            egui::Align2::LEFT_TOP,
            description,
            FontId::proportional(18.667),
            theme::ACCENT,
        );
        panels::source_controls::decorate(ui, &response);
        if response.clicked() {
            ui.ctx()
                .data_mut(|data| data.insert_temp(egui::Id::new("release-notes-open"), true));
        }
        for (label, url, icon) in [
            (
                "Discord",
                "https://ldtk.io/go/discord",
                egui::include_image!("../../assets/icons/discord.svg"),
            ),
            (
                "Online doc",
                "https://ldtk.io/docs",
                egui::include_image!("../../assets/icons/doc.svg"),
            ),
            (
                "JSON format doc",
                "https://ldtk.io/json",
                egui::include_image!("../../assets/icons/json.svg"),
            ),
            (
                "Report a bug",
                "https://github.com/deepnight/ldtk/issues",
                egui::include_image!("../../assets/icons/bug.svg"),
            ),
            (
                "Contact me",
                "mailto:ldtk@deepnight.net",
                egui::include_image!("../../assets/icons/email.svg"),
            ),
        ] {
            let response = left_button(ui, width, 40., icon, label, theme::MEDIUM);
            ui.painter().hline(
                response.rect.x_range(),
                response.rect.bottom(),
                Stroke::new(1_f32, theme::DARK),
            );
            if response
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .clicked()
            {
                ui.ctx().open_url(egui::OpenUrl::new_tab(url));
            }
        }
    }

    fn home_support_dialog(&mut self, ctx: &egui::Context) {
        let id = egui::Id::new("home-support-open");
        if !ctx.data(|data| data.get_temp::<bool>(id).unwrap_or(false)) {
            return;
        }
        let mut close = false;
        let frame = egui::Frame::new()
            .fill(theme::DARK)
            .corner_radius(8)
            .inner_margin(20)
            .shadow(egui::epaint::Shadow {
                offset: [0, 0],
                blur: 48,
                spread: 0,
                color: Color32::BLACK,
            });
        let response=egui::Modal::new(id).area(egui::Modal::default_area(id).default_size(egui::vec2(740.,(ctx.content_rect().height()-40.).max(200.)))).frame(frame)
            .backdrop_color(Color32::from_rgba_unmultiplied(84,93,115,204)).show(ctx,|ui| {
                ui.set_width(700.);
                egui::ScrollArea::vertical().max_height((ctx.content_rect().height()-80.).max(200.)).show(ui,|ui|{
                    let (header,_)=ui.allocate_exact_size(egui::vec2(700.,344.),Sense::hover());
                    let header=Rect::from_min_max(header.min-egui::vec2(20.,20.),header.max+egui::vec2(20.,0.));
                    ui.painter().rect_filled(header,8.,Color32::BLACK);
                    if let Some(texture)=self.assets.builtin(ctx,"home-deepnight",include_bytes!("../../assets/img/deepnight.png")) {
                        paint_contained(ui.painter(),&texture,header.shrink2(egui::vec2(64.,32.)),egui::Align2::CENTER_CENTER);
                    }
                    let exit=Rect::from_min_size(header.right_top()+egui::vec2(-28.,8.),Vec2::splat(20.));
                    egui::Image::new(egui::include_image!("../../assets/icons/close.png")).paint_at(ui,exit);
                    close|=ui.interact(exit,id.with("close"),Sense::click()).clicked();
                    ui.add_space(32.);
                    for title in ["LDtk is \"pay-what-you-want, including free\",","even for commercial projects :)"] {
                        let (rect,_)=ui.allocate_exact_size(egui::vec2(700.,31.),Sense::hover());
                        ui.painter().text(rect.center(),egui::Align2::CENTER_CENTER,locale::tr(ui, title),FontId::new(25.6,egui::FontFamily::Name("heading".into())),theme::ACCENT);
                    }
                    ui.add_space(32.);
                    ui.label(RichText::new(locale::tr(ui, "Everything you will create with it will be fully yours: you are free to use these creations as you wish. If you want to financially support my work, you have various ways to do so:")).size(16.));
                    ui.add_space(32.);
                    ui.horizontal(|ui|{
                        ui.spacing_mut().item_spacing.x=2.;
                        for (index,(label,url)) in [("Buy once on Itch.io","https://deepnight.itch.io/ldtk/purchase"),("Become a GitHub sponsor","https://github.com/sponsors/deepnight"),("Check my Steam games","https://ldtk.io/go/steam")].into_iter().enumerate() {
                            let fill=if index==0{Color32::from_rgb(227,77,0)}else{Color32::from_rgb(42,115,171)};
                            if panels::source_controls::add_sized_button(ui,[232.,56.],egui::Button::new(RichText::new(locale::tr(ui, label)).font(FontId::new(14.667,egui::FontFamily::Name("panel-title".into())))).fill(fill).corner_radius(0).stroke(Stroke::NONE)).clicked(){ctx.open_url(egui::OpenUrl::new_tab(url));}
                        }
                    });
                });
            });
        if close || response.should_close() {
            ctx.data_mut(|data| data.insert_temp(id, false));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn home_frame(
        app: &mut EditorApp,
        ctx: &egui::Context,
        events: Vec<egui::Event>,
    ) -> egui::FullOutput {
        ctx.run(
            egui::RawInput {
                screen_rect: Some(Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1440., 900.),
                )),
                events,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let (rect, _) = ui.allocate_exact_size(ui.available_size(), Sense::hover());
                    app.library_home(ctx, ui, rect);
                });
            },
        )
    }

    fn text_rect(output: &egui::FullOutput, needle: &str) -> Rect {
        output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.job.text == needle => {
                    Some(Rect::from_min_size(text.pos, text.galley.size()))
                }
                _ => None,
            })
            .unwrap_or_else(|| panic!("missing source Home label {needle}"))
    }

    fn click(app: &mut EditorApp, ctx: &egui::Context, point: egui::Pos2) {
        for pressed in [true, false] {
            home_frame(
                app,
                ctx,
                vec![
                    egui::Event::PointerMoved(point),
                    egui::Event::PointerButton {
                        pos: point,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: Default::default(),
                    },
                ],
            );
        }
    }

    #[test]
    fn source_home_search_uses_gray_placeholder_and_source_rounding() {
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let mut app = EditorApp::empty();
        home_frame(&mut app, &ctx, vec![]);
        let output = home_frame(&mut app, &ctx, vec![]);
        let hint = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(t) if t.galley.text() == "Search..." => Some(t),
                _ => None,
            })
            .expect("original Search... placeholder");
        assert_eq!(
            hint.galley.job.sections[0].format.color,
            Color32::from_rgb(117, 117, 117)
        );
        let hint_rect = Rect::from_min_size(hint.pos, hint.galley.size());
        assert!(output.shapes.iter().any(|shape| matches!(&shape.shape, egui::Shape::Rect(r)
            if r.rect.contains_rect(hint_rect) && r.corner_radius==egui::CornerRadius::same(16) && r.stroke.color==theme::LIGHT)));
    }

    #[test]
    fn source_home_search_focus_glow_does_not_hide_the_hint_or_break_filtering() {
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let mut app = EditorApp::empty();
        let dir = tempfile::tempdir().unwrap();
        app.settings.value["recentDirs"] = serde_json::json!([dir.path()]);
        home_frame(&mut app, &ctx, vec![]);
        let id = egui::Id::new("source-home-search").with("input");
        let bounds = ctx.read_response(id).unwrap().rect;
        click(&mut app, &ctx, bounds.center());
        let output = home_frame(&mut app, &ctx, vec![]);
        assert!(ctx.memory(|memory| memory.has_focus(id)));
        let hint = text_rect(&output, "Search...");
        assert!(output.shapes.iter().any(|shape| matches!(&shape.shape, egui::Shape::Rect(r)
            if r.rect.contains_rect(hint) && r.stroke.color==theme::ACCENT && r.corner_radius==egui::CornerRadius::same(16))));
        assert!(
            output
                .shapes
                .iter()
                .any(|shape| matches!(&shape.shape, egui::Shape::Rect(r)
            if r.fill==Color32::from_rgb(227,77,0) && r.blur_width>0.)),
            "focused search needs the source orange glow"
        );
        let output = home_frame(
            &mut app,
            &ctx,
            vec![egui::Event::Text("no-such-folder".into())],
        );
        assert!(!output.shapes.iter().any(|shape| {
            matches!(&shape.shape, egui::Shape::Text(text) if text.galley.job.text == "No recent folder")
        }));
        assert_eq!(
            ctx.data_mut(|data| data.get_temp::<String>(egui::Id::new("source-home-search")))
                .as_deref(),
            Some("no-such-folder")
        );
    }

    #[test]
    fn source_home_places_files_and_about_under_header_and_persists_sample_visibility() {
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let mut app = EditorApp::empty();
        let output = home_frame(&mut app, &ctx, vec![]);
        let examples = text_rect(&output, "Examples");
        let support = text_rect(&output, "Support my work");
        text_rect(&output, "Update 1.5.4");
        assert!(examples.top() > 280. && examples.top() < 345.);
        assert!(support.left() > examples.right() + 400.);
        text_rect(&output, "Example projects to discover LDtk");
        text_rect(&output, "No recent project file");
        text_rect(&output, "No recent folder");
        click(&mut app, &ctx, examples.center());
        assert_eq!(app.settings.ui_state_int("HideSamplesOnHome", None, 0), 1);
        let hidden = home_frame(&mut app, &ctx, vec![]);
        assert!(!hidden.shapes.iter().any(|s| matches!(&s.shape,egui::Shape::Text(t) if t.galley.job.text == "Example projects to discover LDtk")));
    }

    #[test]
    fn recent_folder_click_uses_browse_action_and_search_filters_both_lists() {
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let directory = tempfile::tempdir().unwrap();
        let project_path = directory.path().join("Needle.ldtk");
        let mut app = EditorApp::empty();
        app.set_project(Project::new());
        app.dirty = true;
        app.settings.value["recentProjects"] = serde_json::json!([project_path]);
        app.settings.value["recentDirs"] = serde_json::json!([directory.path()]);
        app.settings.set_ui_state_int("HideSamplesOnHome", None, 1);
        let output = home_frame(&mut app, &ctx, vec![]);
        let folder = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Text(text)
                    if text.galley.job.text == directory.path().to_str().unwrap() =>
                {
                    Some(Rect::from_min_size(text.pos, text.galley.size()))
                }
                _ => None,
            })
            .max_by(|a, b| a.left().total_cmp(&b.left()))
            .unwrap();
        click(&mut app, &ctx, folder.center());
        assert!(
            matches!(&app.pending, Some(Action::BrowseInDir(path)) if path == directory.path()),
            "folder={folder:?} pending={} dirty={} busy={} ui={}",
            app.pending.is_some(),
            app.dirty,
            app.is_busy(),
            app.ui_commit.pending()
        );
        app.pending = None;
        let output = home_frame(&mut app, &ctx, vec![]);
        let search = text_rect(&output, "Search...");
        click(&mut app, &ctx, search.center());
        let filtered = home_frame(
            &mut app,
            &ctx,
            vec![egui::Event::Text("never-matches-this-folder".into())],
        );
        assert!(!filtered.shapes.iter().any(|shape| {
            matches!(&shape.shape, egui::Shape::Text(text)
                if matches!(text.galley.job.text.as_str(), "Needle" | "No recent project file" | "No recent folder")
                    || text.galley.job.text == directory.path().to_str().unwrap())
        }));
    }

    #[test]
    fn source_home_recent_lists_follow_search_without_extra_blank_space() {
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let mut app = EditorApp::empty();
        app.settings.value["recentDirs"] = serde_json::json!([std::env::temp_dir()]);
        home_frame(&mut app, &ctx, vec![]);
        let output = home_frame(&mut app, &ctx, vec![]);
        let search = text_rect(&output, "Search...");
        let recent = text_rect(&output, "Recent folders");
        assert!(
            recent.top() - search.bottom() < 24.,
            "extra Home gap: search={search:?} recent={recent:?}"
        );
    }

    #[test]
    fn home_search_can_be_cleared_with_escape_or_the_clear_button() {
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let mut app = EditorApp::empty();
        app.settings.value["recentProjects"] = serde_json::json!(["Needle.ldtk", "Other.ldtk"]);
        app.settings.set_ui_state_int("HideSamplesOnHome", None, 1);
        let search = text_rect(&home_frame(&mut app, &ctx, vec![]), "Search...");
        click(&mut app, &ctx, search.center());
        let filtered = home_frame(&mut app, &ctx, vec![egui::Event::Text("Needle".into())]);
        text_rect(&filtered, "Needle");
        assert!(!filtered.shapes.iter().any(|shape| {
            matches!(&shape.shape, egui::Shape::Text(text)
                if matches!(text.galley.job.text.as_str(), "Other" | "Recent projects"))
        }));
        let output = home_frame(
            &mut app,
            &ctx,
            vec![egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: Some(egui::Key::Escape),
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        text_rect(&output, "Other");
        text_rect(&output, "Recent projects");
        assert!(ctx.memory(|memory| {
            memory.has_focus(egui::Id::new("source-home-search").with("input"))
        }));
        home_frame(&mut app, &ctx, vec![egui::Event::Text("Needle".into())]);
        let clear = ctx
            .read_response(egui::Id::new("source-home-search").with("clear"))
            .expect("a nonempty search shows its clear button");
        click(&mut app, &ctx, clear.rect.center());
        text_rect(&home_frame(&mut app, &ctx, vec![]), "Other");
    }

    #[test]
    fn source_home_paths_use_components_with_a_bold_final_folder() {
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let mut app = EditorApp::empty();
        let dir = tempfile::tempdir().unwrap();
        app.settings.value["recentDirs"] = serde_json::json!([dir.path()]);
        let output = home_frame(&mut app, &ctx, vec![]);
        let path = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(t) if t.galley.text() == dir.path().to_str().unwrap() => Some(t),
                _ => None,
            })
            .expect("recent folder path");
        assert!(
            path.galley.job.sections.len() >= 3,
            "the original styles slash separators and path components separately"
        );
        let last = &path.galley.job.sections.last().unwrap().format;
        assert_eq!(
            last.font_id.family,
            egui::FontFamily::Name("body-bold".into())
        );
        assert!((last.font_id.size - 13.333).abs() < 0.01);
        assert_eq!(last.color, Color32::WHITE);
    }

    #[test]
    fn source_home_missing_folder_marks_separators_and_components_as_missing() {
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("deleted-folder");
        let mut app = EditorApp::empty();
        app.settings.value["recentDirs"] = serde_json::json!([missing]);
        let output = home_frame(&mut app, &ctx, vec![]);
        let path = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(t) if t.galley.text() == missing.to_str().unwrap() => Some(t),
                _ => None,
            })
            .expect("missing folder path");
        for section in &path.galley.job.sections {
            assert_eq!(section.format.color, Color32::from_rgb(255, 163, 134));
            assert!(section.format.strikethrough.width > 0.);
        }
    }

    #[test]
    fn source_home_context_button_fills_the_project_row_and_keeps_clicks_in_menu() {
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Full_height_menu.ldtk");
        Project::new().save(&path).unwrap();
        let mut app = EditorApp::empty();
        app.settings.value["recentProjects"] = serde_json::json!([path]);
        app.dirty = true;
        home_frame(&mut app, &ctx, vec![]);
        let output = home_frame(&mut app, &ctx, vec![]);
        let title = text_rect(&output, "Full_height_menu");
        let search = text_rect(&output, "Search...");
        let panel = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Rect(r)
                    if r.stroke.color == theme::LIGHT
                        && r.rect.top() > search.bottom()
                        && r.rect.width() > 300. =>
                {
                    Some(r.rect)
                }
                _ => None,
            })
            .expect("recent project list bounds");
        click(
            &mut app,
            &ctx,
            egui::pos2(panel.right() - 21., title.top() + 34.),
        );
        let output = home_frame(&mut app, &ctx, vec![]);
        let load = text_rect(&output, "LOAD FROM THIS FOLDER");
        let locate = text_rect(&output, "LOCATE FILE");
        let color = text_rect(&output, "ASSIGN CUSTOM COLOR");
        let remove = text_rect(&output, "REMOVE FROM HISTORY");
        assert!((locate.top() - load.top() - 38.5).abs() <= 1., "{load:?} {locate:?}");
        assert!((color.top() - locate.top() - 47.).abs() <= 1., "{locate:?} {color:?}");
        assert!((locate.left() - remove.left() - 30.).abs() < 1.);
        assert!(
            app.pending.is_none() && app.project.is_none(),
            "context menu click must not open the row"
        );
        click(&mut app, &ctx, remove.center());
        assert_eq!(app.settings.value["recentProjects"], serde_json::json!([]));
        assert!(path.is_file());
    }

    #[test]
    fn source_home_sample_close_button_fits_inside_the_examples_frame() {
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let mut app = EditorApp::empty();
        home_frame(&mut app, &ctx, vec![]);
        let output = home_frame(&mut app, &ctx, vec![]);
        let panel = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Rect(r) if r.stroke.color == HELP && r.rect.width() > 500. => {
                    Some(r.rect)
                }
                _ => None,
            })
            .expect("examples panel");
        let close = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Rect(r) if r.fill == HELP_DARK && r.rect.width() < 50. => Some(r.rect),
                _ => None,
            })
            .expect("examples close button");
        assert!(
            panel.shrink(1.).contains_rect(close),
            "clipped close control: panel={panel:?} button={close:?}"
        );
        assert!(
            close.width() <= 27. && close.height() <= 27.,
            "source icon button must not inherit general button padding: {close:?}"
        );
    }

    #[test]
    fn support_entrance_opens_original_author_choices_without_navigation() {
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let mut app = EditorApp::empty();
        let output = home_frame(&mut app, &ctx, vec![]);
        click(
            &mut app,
            &ctx,
            text_rect(&output, "Support my work").center(),
        );
        home_frame(&mut app, &ctx, vec![]);
        let output = home_frame(&mut app, &ctx, vec![]);
        text_rect(&output, "even for commercial projects :)");
        text_rect(&output, "Buy once on Itch.io");
        text_rect(&output, "Become a GitHub sponsor");
        text_rect(&output, "Check my Steam games");
        assert!(app.project.is_none());
    }
}
