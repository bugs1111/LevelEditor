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
        egui::Button::image_and_text(
            egui::Image::new(image).fit_to_exact_size(Vec2::splat(25.)),
            RichText::new(label).font(font),
        )
        .fill(color)
        .stroke(Stroke::NONE)
        .corner_radius(0),
    );
    if label == "Examples" {
        egui::Image::new(egui::include_image!("../../assets/icons/expanded.svg")).paint_at(
            ui,
            Rect::from_center_size(
                response.rect.center() + egui::vec2(60., 0.),
                egui::vec2(16., 16.),
            ),
        );
    }
    response
}

fn left_button(
    ui: &mut egui::Ui,
    width: f32,
    height: f32,
    image: egui::ImageSource<'static>,
    label: &str,
    color: Color32,
) -> egui::Response {
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

fn matches_search(path: &std::path::Path, query: &str) -> bool {
    let path = path.to_string_lossy().to_lowercase();
    query
        .split_whitespace()
        .all(|word| path.contains(&word.to_lowercase()))
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
            let flexible=(ui.available_width()-88.-38.-if fullscreen {38.+2.} else {0.}-8.)/3.;
            if button(ui,flexible,egui::include_image!("../../assets/icons/tutorial.svg"),"Examples",Color32::from_rgb(73,141,85)).on_hover_text("These examples showcase various functionalities from LDtk editor, including auto-layer rules and entities.").clicked() { hide = !hide; }
            if button(ui,flexible,egui::include_image!("../../assets/icons/new.svg"),"New",Color32::from_rgb(227,77,0)).clicked() { action=Some(Action::New); }
            if button(ui,flexible,egui::include_image!("../../assets/icons/open.svg"),"Load",Color32::from_rgb(227,77,0)).clicked() { action=Some(Action::Browse); }
            let import=button(ui,88.,egui::include_image!("../../assets/icons/open.svg"),"Import",theme::LIGHT);
            egui::Popup::menu(&import).show(|ui| {
                ui.weak("Import a project from another app");
                if ui.button("Ogmo 3 project").clicked() { action=Some(Action::BrowseOgmo);ui.close(); }
            });
            if button(ui,38.,egui::include_image!("../../assets/icons/settings.svg"),"",theme::LIGHT).on_hover_text("Application settings").clicked() { self.show_panel(Some(Panel::Settings)); }
            if fullscreen && button(ui,38.,egui::include_image!("../../assets/icons/exit.svg"),"",theme::DARK).on_hover_text("Close app").clicked() { action=Some(Action::Quit); }
        });
        if !hide {
            self.home_samples(ctx, &mut ui, &mut hide, &mut action);
        }
        if hide != initially_hidden {
            self.settings
                .set_ui_state_int("HideSamplesOnHome", None, i64::from(hide));
        }
        let search_id = egui::Id::new("source-home-search");
        let mut search = ctx
            .data_mut(|data| data.get_temp::<String>(search_id))
            .unwrap_or_default();
        let search_widgets = ui.visuals().widgets.clone();
        ui.visuals_mut().widgets.inactive.corner_radius = egui::CornerRadius::same(100);
        ui.visuals_mut().widgets.hovered.corner_radius = egui::CornerRadius::same(100);
        ui.visuals_mut().widgets.active.corner_radius = egui::CornerRadius::same(100);
        let search_response = ui.add_sized(
            [ui.available_width(), 33.],
            egui::TextEdit::singleline(&mut search)
                .id(search_id.with("input"))
                .hint_text("Search…")
                .margin(egui::Margin {
                    left: 28,
                    right: 8,
                    top: 8,
                    bottom: 8,
                }),
        );
        ui.visuals_mut().widgets = search_widgets;
        egui::Image::new(egui::include_image!("../../assets/icons/search.svg")).paint_at(
            &ui,
            Rect::from_center_size(
                search_response.rect.left_center() + egui::vec2(16., 0.),
                Vec2::splat(16.),
            ),
        );
        ctx.data_mut(|data| data.insert_temp(search_id, search.clone()));
        // CSS keeps a32pt gap between Search and the recent lists.
        let list_rect = Rect::from_min_max(ui.cursor().min + egui::vec2(0., 32.), files.max);
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
            "If you read this, you're awesome! Have a nice day <3",
            FontId::proportional(12.),
            theme::LIGHT,
        );
        self.home_support_dialog(ctx);
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
                ui.horizontal(|ui| {
                    let (title, _) = ui.allocate_exact_size(
                        egui::vec2(ui.available_width() - 30., 24.),
                        Sense::hover(),
                    );
                    ui.painter().text(
                        title.left_center() + egui::vec2(4., 0.),
                        egui::Align2::LEFT_CENTER,
                        "Example projects to discover LDtk",
                        FontId::proportional(13.333),
                        HELP,
                    );
                    if ui
                        .add_sized(
                            [22., 22.],
                            egui::Button::image(
                                egui::Image::new(egui::include_image!(
                                    "../../assets/icons/close.png"
                                ))
                                .fit_to_exact_size(Vec2::splat(16.)),
                            )
                            .fill(HELP_DARK),
                        )
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
                        ui.spacing_mut().scroll = egui::style::ScrollStyle::solid();
                        ui.spacing_mut().scroll.bar_width = 15.;
                        ui.spacing_mut().scroll.bar_inner_margin = 0.;
                        ui.spacing_mut().scroll.bar_outer_margin = 0.;
                        egui::ScrollArea::horizontal()
                            .id_salt("source-home-samples")
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
                                        ui.painter().rect_filled(rect, 4., Color32::BLACK);
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
                                            ui.painter().image(texture.id(), thumb, uv, tint);
                                        }
                                        let text_rect = Rect::from_min_max(
                                            thumb.left_bottom() + egui::vec2(2., 4.),
                                            rect.right_bottom() - egui::vec2(2., 4.),
                                        );
                                        let galley = ui.painter().layout(
                                            name.replace('_', " "),
                                            FontId::proportional(11.333),
                                            tint,
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
        let entries: Vec<PathBuf> = entries
            .iter()
            .filter_map(Value::as_str)
            .map(PathBuf::from)
            .filter(|path| matches_search(path, query))
            .collect();
        let rect = ui.max_rect();
        ui.painter().rect_filled(rect, 0., theme::DARK);
        ui.painter().rect_stroke(
            rect,
            0.,
            Stroke::new(1_f32, theme::LIGHT),
            egui::StrokeKind::Inside,
        );
        if entries.is_empty() {
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
                    "No recent project file"
                } else {
                    "No recent folder"
                },
                FontId::new(20.8, egui::FontFamily::Name("panel-title".into())),
                Color32::from_rgb(115, 127, 157),
            );
            return;
        }
        let mut content = child(ui, rect.shrink(4.), "source-recent-inner");
        egui::ScrollArea::vertical()
            .id_salt(if files { "files" } else { "folders" })
            .max_height(rect.height() - 8.)
            .show(&mut content, |ui| {
                ui.set_width(rect.width() - 8.);
                ui.label(
                    RichText::new(if files {
                        "Recent projects"
                    } else {
                        "Recent folders"
                    })
                    .font(FontId::new(
                        12.,
                        egui::FontFamily::Name("panel-title".into()),
                    ))
                    .color(theme::ACCENT),
                );
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
                            if files && !sample { 46. } else { 28. },
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
                    let text_rect = Rect::from_min_max(
                        row.min + egui::vec2(8., if files && !sample { 5. } else { 3. }),
                        egui::pos2(row.right() - if files { 82. } else { 24. }, row.bottom()),
                    );
                    let font = FontId::new(
                        16.,
                        egui::FontFamily::Name(
                            if files { "heading" } else { "panel-title" }.into(),
                        ),
                    );
                    let mut job = egui::text::LayoutJob::simple(
                        title.to_string(),
                        font,
                        text_color,
                        text_rect.width(),
                    );
                    job.wrap.max_rows = 1;
                    job.wrap.break_anywhere = true;
                    if missing {
                        job.sections[0].format.strikethrough = Stroke::new(1_f32, text_color);
                    }
                    let galley = ui.fonts_mut(|fonts| fonts.layout_job(job));
                    ui.painter().galley(text_rect.min, galley, text_color);
                    if files && !sample {
                        let path_color =
                            mix_color(theme::DARK, mix_color(color, Color32::WHITE, 0.6), 0.55);
                        let mut job = egui::text::LayoutJob::simple(
                            directory.to_string_lossy().into_owned(),
                            FontId::proportional(12.),
                            path_color,
                            text_rect.width(),
                        );
                        job.wrap.max_rows = 1;
                        job.wrap.break_anywhere = true;
                        let galley = ui.fonts_mut(|fonts| fonts.layout_job(job));
                        ui.painter().galley(
                            egui::pos2(text_rect.left(), row.top() + 25.),
                            galley,
                            path_color,
                        );
                    }
                    let mut menu = child(
                        ui,
                        Rect::from_min_max(egui::pos2(row.right() - 22., row.top()), row.max),
                        &format!("recent-context-{}", path.display()),
                    );
                    menu.spacing_mut().button_padding = Vec2::ZERO;
                    menu.menu_button(
                        egui::Image::new(egui::include_image!(
                            "../../assets/icons/contextMenu.svg"
                        ))
                        .fit_to_exact_size(Vec2::splat(16.)),
                        |ui| {
                            if files {
                                self.recent_file_menu(ui, &path);
                            } else {
                                self.recent_folder_menu(ui, &path);
                            }
                        },
                    );
                    if files {
                        let mut backup = child(
                            ui,
                            Rect::from_min_size(
                                egui::pos2(row.right() - 78., row.top() + 5.),
                                egui::vec2(54., row.height() - 10.),
                            ),
                            &format!("recent-backups-{}", path.display()),
                        );
                        self.home_backups_menu(&mut backup, &path);
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
                    response.context_menu(|ui| {
                        if files {
                            self.recent_file_menu(ui, &path);
                        } else {
                            self.recent_folder_menu(ui, &path);
                        }
                    });
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
        let (rect, response) = ui.allocate_exact_size(egui::vec2(width, 96.), Sense::click());
        ui.painter().rect_filled(rect, 0., theme::LIGHT);
        ui.painter().text(
            rect.min + egui::vec2(8., 17.),
            egui::Align2::LEFT_TOP,
            "Update 1.5.3",
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
            "Release notes",
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
                        ui.painter().text(rect.center(),egui::Align2::CENTER_CENTER,title,FontId::new(25.6,egui::FontFamily::Name("heading".into())),theme::ACCENT);
                    }
                    ui.add_space(32.);
                    ui.label(RichText::new("Everything you will create with it will be fully yours: you are free to use these creations as you wish. If you want to financially support my work, you have various ways to do so:").size(16.));
                    ui.add_space(32.);
                    ui.horizontal(|ui|{
                        ui.spacing_mut().item_spacing.x=2.;
                        for (index,(label,url)) in [("Buy once on Itch.io","https://deepnight.itch.io/ldtk/purchase"),("Become a GitHub sponsor","https://github.com/sponsors/deepnight"),("Check my Steam games","https://ldtk.io/go/steam")].into_iter().enumerate() {
                            let fill=if index==0{Color32::from_rgb(227,77,0)}else{Color32::from_rgb(42,115,171)};
                            if panels::source_controls::add_sized_button(ui,[232.,56.],egui::Button::new(RichText::new(label).font(FontId::new(14.667,egui::FontFamily::Name("panel-title".into())))).fill(fill).corner_radius(0).stroke(Stroke::NONE)).clicked(){ctx.open_url(egui::OpenUrl::new_tab(url));}
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
    fn source_home_places_files_and_about_under_header_and_persists_sample_visibility() {
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let mut app = EditorApp::empty();
        let output = home_frame(&mut app, &ctx, vec![]);
        let examples = text_rect(&output, "Examples");
        let support = text_rect(&output, "Support my work");
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
        let search = text_rect(&output, "Search…");
        click(&mut app, &ctx, search.center());
        let filtered = home_frame(
            &mut app,
            &ctx,
            vec![egui::Event::Text("never-matches-this-folder".into())],
        );
        assert!(!filtered.shapes.iter().any(|s| matches!(&s.shape,egui::Shape::Text(t) if t.galley.job.text == "Needle" || t.galley.job.text == directory.path().to_str().unwrap())));
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
