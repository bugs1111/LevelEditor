//! The image picker shared by tilesets and level backgrounds.
use super::*;
use crate::app::chrome::ActionTooltip as _;

const IMAGE_BUTTON_HEIGHT: f32 = 32.;

fn recall_image(button: &egui::Response, images: &[PathBuf]) -> Option<PathBuf> {
    let window = button.ctx.content_rect();
    let align_right = button.rect.left() >= window.left() + window.width() * 0.6;
    let open_above = button.rect.top() >= window.top() + window.height() * 0.7;
    let align = match (open_above, align_right) {
        (false, false) => egui::RectAlign::BOTTOM_START,
        (false, true) => egui::RectAlign::BOTTOM_END,
        (true, false) => egui::RectAlign::TOP_START,
        (true, true) => egui::RectAlign::TOP_END,
    };
    egui::Popup::menu(button)
        .align(align)
        .frame(
            egui::Frame::new()
                .fill(theme::DARK)
                .stroke(Stroke::new(1_f32, Color32::BLACK))
                .shadow(egui::Shadow {
                    offset: [0, 4],
                    blur: 16,
                    spread: 0,
                    color: Color32::from_black_alpha(180),
                }),
        )
        .show(|ui| {
            let font = FontId::proportional(13.333);
            let labels = images
                .iter()
                .map(|path| {
                    ui.painter().layout_no_wrap(
                        file_label(path).to_uppercase(),
                        font.clone(),
                        theme::ACCENT,
                    )
                })
                .collect::<Vec<_>>();
            let width = labels
                .iter()
                .map(|label| label.size().x + 40.)
                .fold(0., f32::max)
                .min((window.width() - 32.).max(20.));
            ui.spacing_mut().item_spacing.y = 0.;
            egui::ScrollArea::vertical()
                .max_height((window.height() - 32.).max(32.))
                .show(ui, |ui| {
                    let mut chosen = None;
                    for (path, label) in images.iter().zip(labels) {
                        let (rect, response) =
                            ui.allocate_exact_size(egui::vec2(width, 32.), Sense::click());
                        response.widget_info(|| {
                            egui::WidgetInfo::labeled(
                                egui::WidgetType::Button,
                                true,
                                file_label(path),
                            )
                        });
                        let color = if response.hovered() || response.has_focus() {
                            ui.painter().rect_filled(rect, 0., theme::ACCENT);
                            Color32::BLACK
                        } else {
                            theme::ACCENT
                        };
                        ui.painter()
                            .with_clip_rect(rect)
                            .galley_with_override_text_color(
                                rect.left_center() + egui::vec2(20., -label.size().y * 0.5),
                                label,
                                color,
                            );
                        if response
                            .detail_tooltip(path.display().to_string())
                            .clicked()
                        {
                            chosen = Some(path.clone());
                        }
                    }
                    if chosen.is_some() {
                        ui.close();
                    }
                    chosen
                })
                .inner
        })
        .and_then(|menu| menu.inner)
}

fn icon_button(
    ui: &mut egui::Ui,
    icon: egui::ImageSource<'static>,
    label: &str,
    fill: Color32,
) -> egui::Response {
    source_controls::add_sized_button(
        ui,
        [30., IMAGE_BUTTON_HEIGHT],
        egui::Button::image(egui::Image::new(icon).fit_to_exact_size(Vec2::splat(16.))).fill(fill),
    )
    .action_tooltip(locale::tr(ui, label))
}

impl EditorApp {
    /// A changed path is returned to the form so it can apply its own document transaction.
    pub(super) fn source_image_picker(
        &mut self,
        ui: &mut egui::Ui,
        owner: impl std::hash::Hash,
        relative: Option<&str>,
    ) -> Option<Option<PathBuf>> {
        let project = self.project.as_ref()?;
        let id = egui::Id::new((
            "image-picker-remove",
            text(&project.json, "iid", ""),
            owner,
            relative,
        ));
        let current = relative.map(|path| canvas::asset_path(project, path));
        let mut images = self
            .assets
            .loaded_image_paths()
            .map(super::resources::clean)
            .collect::<Vec<_>>();
        if let Some(path) = current.as_ref().filter(|path| path.is_file()) {
            images.push(super::resources::clean(path));
        }
        images.sort_by_cached_key(|path| (file_label(path).to_lowercase(), path.clone()));
        images.dedup();
        let mut change = None;
        let orange = Color32::from_rgb(235, 78, 0);
        let dark = ui.visuals().widgets.inactive.bg_fill;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.;
            ui.spacing_mut().button_padding = egui::vec2(4., 2.);
            if let Some(path) = &current
                && icon_button(
                    ui,
                    egui::include_image!("../../assets/icons/refresh.svg"),
                    "Manually reload file",
                    orange,
                )
                .clicked()
            {
                self.retry_resource(ui.ctx(), path);
            }
            let label = match &current {
                Some(path) if path.is_file() => file_label(path),
                Some(_) => locale::tr(ui, "File not found!"),
                None => locale::tr(ui, "[ No image ]"),
            };
            let reserve =
                if current.is_some() { 64. } else { 0. } + if images.is_empty() { 0. } else { 32. };
            let empty = current.is_none();
            let size = egui::vec2(
                (ui.available_width() - reserve).max(20.),
                if empty && images.is_empty() {
                    24.
                } else {
                    IMAGE_BUTTON_HEIGHT
                },
            );
            let pick = ui
                .scope(|ui| {
                    let fill = if empty {
                        let rect = Rect::from_min_size(ui.next_widget_position(), size);
                        if ui.is_enabled() && ui.rect_contains_pointer(rect) {
                            Color32::from_rgb(115, 127, 157)
                        } else {
                            ui.multiply_opacity(0.6);
                            theme::LIGHT
                        }
                    } else if current.as_ref().is_some_and(|path| !path.is_file()) {
                        Color32::DARK_RED
                    } else {
                        orange
                    };
                    source_controls::add_sized_button(
                        ui,
                        size,
                        egui::Button::new(RichText::new(label).color(Color32::WHITE))
                            .truncate()
                            .fill(fill),
                    )
                })
                .inner;
            let pick = match &current {
                Some(path) => pick.detail_tooltip(path.display().to_string()),
                None => pick.action_tooltip(locale::tr(ui, "Pick an image")),
            };
            if pick.clicked() {
                let project_path = self.project.as_ref().and_then(|p| p.path.clone());
                let mut dialog = FileDialog::new().add_filter(
                    locale::tr(ui, "Images"),
                    &["png", "gif", "jpg", "jpeg", "aseprite", "ase"],
                );
                if let Some(dir) = self.settings.ui_dir(
                    project_path.as_deref(),
                    "PickImage",
                    current.as_deref().and_then(Path::parent),
                ) {
                    dialog = dialog.set_directory(dir);
                }
                if let Some(path) = dialog.pick_file() {
                    if let Some(dir) = path.parent() {
                        self.settings
                            .store_ui_dir(project_path.as_deref(), "PickImage", dir);
                    }
                    change = Some(Some(path));
                }
            }
            if !images.is_empty() {
                let recall = icon_button(
                    ui,
                    egui::include_image!("../../assets/icons/recall.svg"),
                    "Use an existing image",
                    orange,
                );
                if let Some(path) = recall_image(&recall, &images) {
                    change = Some(Some(path));
                }
            }
            if let Some(path) = &current {
                let remove = icon_button(
                    ui,
                    egui::include_image!("../../assets/icons/clear.svg"),
                    "Stop using this image",
                    dark,
                );
                if remove.clicked() {
                    ui.ctx().data_mut(|d| d.insert_temp(id, remove.rect));
                }
                if icon_button(
                    ui,
                    egui::include_image!("../../assets/icons/locate.svg"),
                    "Locate file",
                    dark,
                )
                .clicked()
                    && let Err(error) = home_files::locate(path, true)
                {
                    self.status = error.to_string();
                    self.status_error = true;
                }
            }
        });
        if let Some(anchor) = ui.ctx().data(|d| d.get_temp::<Rect>(id)) {
            let window = ui.ctx().content_rect();
            let left = anchor.left() < window.left() + window.width() * 0.6;
            let below = anchor.top() < window.top() + window.height() * 0.7;
            let area = egui::Area::new(id)
                .kind(egui::UiKind::Modal)
                .sense(Sense::hover())
                .order(egui::Order::Foreground)
                .pivot(egui::Align2([
                    if left {
                        egui::Align::Min
                    } else {
                        egui::Align::Max
                    },
                    if below {
                        egui::Align::Min
                    } else {
                        egui::Align::Max
                    },
                ]))
                .fixed_pos(egui::pos2(
                    if left { anchor.left() } else { anchor.right() },
                    if below { anchor.bottom() } else { anchor.top() },
                ));
            let modal = project_dialogs::form_warning_dialog(id)
                .area(area)
                .show(ui.ctx(), |ui| {
                    let question = locale::tr(ui, "Stop using this image?");
                    let width = project_dialogs::confirmation_prompt(ui, question);
                    ui.horizontal(|ui| {
                        let size = egui::vec2((width - 2.) * 0.5, 35.);
                        let confirm =
                            project_dialogs::button(ui, "CONFIRM", size, false, 13.333).clicked();
                        let cancel =
                            project_dialogs::button(ui, "CANCEL", size, true, 13.333).clicked();
                        (confirm, cancel)
                    })
                    .inner
                });
            let enter = modal.is_top_modal
                && ui
                    .ctx()
                    .input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Enter));
            let confirmed = modal.inner.0 || enter;
            if confirmed {
                change = Some(None);
            }
            if confirmed || modal.inner.1 || modal.should_close() {
                ui.ctx().data_mut(|d| d.remove::<Rect>(id));
            }
        }
        if let Some(Some(path)) = &change
            && current.as_ref() == Some(path)
        {
            self.retry_resource(ui.ctx(), path);
            return None;
        }
        change
    }
}

fn file_label(path: &Path) -> &str {
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saving_discards_unused_images_from_recall_but_keeps_other_levels_and_tilesets() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Project.ldtk");
        let files =
            ["Old.png", "Active.png", "Other.png", "Atlas.png"].map(|name| dir.path().join(name));
        let pixels = image::RgbaImage::from_pixel(16, 16, image::Rgba([10, 20, 30, 255]));
        for file in &files {
            pixels.save(file).unwrap();
        }
        let mut project = Project::new();
        project.path = Some(path.clone());
        let key = project.level_keys()[0];
        let other = project.add_level(None, "Other", 256, 256).unwrap();
        project.level_mut(key).unwrap()["bgRelPath"] = json!("Active.png");
        project.level_mut(other).unwrap()["bgRelPath"] = json!("./Other.png");
        project.add_tileset(&files[3], 16, 16, 16).unwrap();
        let mut app = EditorApp::empty();
        app.set_project(project);
        let ctx = egui::Context::default();
        for file in &files {
            app.assets
                .store_file_image(&ctx, file, pixels.clone())
                .unwrap();
        }
        assert!(app.assets.loaded_image_paths().any(|file| file == files[0]));

        app.save_async_at(path.clone(), None);
        let deadline = Instant::now() + std::time::Duration::from_secs(5);
        while app.saving.is_some() {
            assert!(Instant::now() < deadline, "project save must finish");
            app.poll_services(&ctx);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let saved = Project::load(path).unwrap();
        assert_eq!(saved.level(key).unwrap()["bgRelPath"], "Active.png");
        let mut paths = app.assets.loaded_image_paths().collect::<Vec<_>>();
        paths.sort();
        assert_eq!(paths, [&files[1], &files[3], &files[2]]);
        assert!(files[0].is_file(), "saving only prunes the cache");
    }

    #[test]
    fn image_removal_only_dispatches_after_confirmation() {
        let dir = tempfile::tempdir().unwrap();
        let image_path = dir.path().join("Atlas.png");
        image::RgbaImage::new(16, 16).save(&image_path).unwrap();
        let mut project = Project::new();
        project.path = Some(dir.path().join("Project.ldtk"));
        let uid = project.add_tileset(&image_path, 16, 16, 16).unwrap();
        let mut app = EditorApp::empty();
        app.set_project(project);
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        ctx.style_mut(|style| style.animation_time = 0.);
        let remove_pos = std::cell::Cell::new(egui::Pos2::ZERO);
        let mut draw = |events| {
            ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(700., 500.),
                    )),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        remove_pos.set(egui::pos2(
                            ui.max_rect().right() - 47.,
                            ui.next_widget_position().y + 15.,
                        ));
                        let path = app.project.as_ref().unwrap().tileset(uid).unwrap()["relPath"]
                            .as_str()
                            .map(str::to_owned);
                        if app.source_image_picker(ui, ("tileset", uid), path.as_deref())
                            == Some(None)
                        {
                            app.edit("Tileset image removed", |p| p.remove_tileset_image(uid));
                        }
                    });
                },
            )
        };
        fn label(shape: &egui::Shape, name: &str) -> Option<egui::Pos2> {
            match shape {
                egui::Shape::Text(t) if t.galley.job.text == name => {
                    Some(t.pos + t.galley.size() * 0.5)
                }
                egui::Shape::Vec(shapes) => shapes.iter().find_map(|s| label(s, name)),
                _ => None,
            }
        }
        let pointer = |pos, pressed| {
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ]
        };
        draw(vec![]);
        let viewport_remove = remove_pos.get();
        for pressed in [true, false] {
            draw(pointer(viewport_remove, pressed));
        }
        let out = draw(vec![]);
        let warning = project_dialogs::form_warning_dialog(egui::Id::NULL)
            .frame
            .unwrap();
        assert!(out.shapes.iter().any(|shape| {
            let egui::Shape::Vec(shapes) = &shape.shape else {
                return false;
            };
            shapes.iter().any(|shape| matches!(shape,
                egui::Shape::Rect(rect) if rect.fill == warning.fill && rect.stroke == warning.stroke
            ))
        }));
        let cancel = out
            .shapes
            .iter()
            .find_map(|s| label(&s.shape, "CANCEL"))
            .expect("removal confirmation");
        for pressed in [true, false] {
            draw(pointer(cancel, pressed));
        }
        let out = draw(vec![]);
        assert!(
            out.shapes
                .iter()
                .any(|s| label(&s.shape, "Atlas.png").is_some())
        );
        for pressed in [true, false] {
            draw(pointer(viewport_remove, pressed));
        }
        let out = draw(vec![]);
        let confirm = out
            .shapes
            .iter()
            .find_map(|s| label(&s.shape, "CONFIRM"))
            .expect("removal confirmation");
        for pressed in [true, false] {
            draw(pointer(confirm, pressed));
        }
        assert!(app.project.as_ref().unwrap().tileset(uid).unwrap()["relPath"].is_null());
        assert!(app.history.undo(app.project.as_mut().unwrap()));
        assert_eq!(
            app.project.as_ref().unwrap().tileset(uid).unwrap()["relPath"],
            "Atlas.png"
        );
    }
}
