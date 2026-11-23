use super::*;
use ldtk_core::SaveStage;
use std::time::Duration;

// The original save meter advances through prechecks, hooks, backup, and the file stages.
const SAVE_STAGES: usize = 9;
const COMPLETION_HOLD: Duration = Duration::from_millis(200);
const FRAME_INTERVAL: Duration = Duration::from_nanos(16_666_667);

#[derive(Clone, Default)]
pub(super) struct SaveProgress {
    completed_stages: usize,
    task: Option<SaveTask>,
    displayed_ratio: f32,
    completed_at: Option<Instant>,
}

#[derive(Clone)]
struct SaveTask {
    stage: SaveStage,
    done: usize,
    total: usize,
    label: Option<String>,
}

impl SaveProgress {
    pub(super) fn record(
        &mut self,
        stage: SaveStage,
        done: usize,
        total: usize,
        label: Option<&str>,
        names: &[String],
    ) {
        if matches!(
            stage,
            SaveStage::PrepareLevels
                | SaveStage::ExportPng
                | SaveStage::ExportSimplified
                | SaveStage::ExportTiled
        ) && total == 0
        {
            // Keep the last task visible while its staged files are being published.
            return;
        }
        self.completed_stages = match stage {
            SaveStage::Backup => 4,
            SaveStage::WriteProject if done == 0 => 6,
            SaveStage::PrepareLevels => 7,
            SaveStage::RemoveObsoleteLevels => 8,
            _ => SAVE_STAGES,
        };
        self.task = (total > 0 && (stage != SaveStage::WriteProject || done == 0)).then(|| {
            let label = label.map(str::to_owned).or_else(|| match stage {
                SaveStage::PrepareLevels | SaveStage::ExportPng if total == names.len() => {
                    names.get(done).cloned()
                }
                SaveStage::ExportSimplified if total == names.len() * 3 && done < total => {
                    names.get(done % names.len()).cloned()
                }
                _ => None,
            });
            SaveTask {
                stage,
                done,
                total,
                label,
            }
        });
    }

    fn meter_visible(&self, now: Instant) -> bool {
        self.completed_at
            .is_none_or(|completed| now.saturating_duration_since(completed) < COMPLETION_HOLD)
    }

    fn target_ratio(&self) -> f32 {
        self.completed_stages as f32 / SAVE_STAGES as f32
    }

    fn animate_meter(&mut self, now: Instant) -> bool {
        let complete = self.completed_stages == SAVE_STAGES;
        if complete {
            self.completed_at.get_or_insert(now);
        }
        if !self.meter_visible(now) {
            return false;
        }
        let target = self.target_ratio();
        let step = if complete { 0.6 } else { 0.3 };
        self.displayed_ratio += (target - self.displayed_ratio) * step;
        if (target - self.displayed_ratio).abs() <= 0.0001 {
            self.displayed_ratio = target;
        }
        true
    }

    pub(super) fn finish(&mut self, now: Instant) {
        self.task = None;
        self.completed_stages = SAVE_STAGES;
        if self.meter_visible(now) {
            self.completed_at = Some(now);
        }
    }

    pub(super) fn title_progress(&self, now: Instant) -> Option<(usize, usize)> {
        if self.meter_visible(now) {
            Some((self.completed_stages, SAVE_STAGES))
        } else {
            self.export_task().map(|task| (task.done, task.total))
        }
    }

    fn export_task(&self) -> Option<&SaveTask> {
        self.task.as_ref().filter(|task| {
            matches!(
                task.stage,
                SaveStage::ExportPng | SaveStage::ExportSimplified | SaveStage::ExportTiled
            )
        })
    }

    pub(super) fn show(&self, ctx: &egui::Context, path: &Path, now: Instant) {
        let meter_visible = self.meter_visible(now);
        if !meter_visible && self.task.is_none() {
            return;
        }
        let name = path
            .file_name()
            .unwrap_or(path.as_os_str())
            .to_string_lossy();
        let width = ctx.content_rect().width().min(600.);
        let id = egui::Id::new("save-progress");
        let area = egui::Modal::default_area(id)
            .anchor(
                Align2::CENTER_TOP,
                egui::vec2(0., if meter_visible { 0. } else { 68. }),
            )
            .default_width(width)
            .fade_in(false);
        egui::Modal::new(id)
            .area(area)
            .frame(egui::Frame::NONE)
            .backdrop_color(if self.task.is_some() {
                Color32::from_black_alpha(55)
            } else {
                Color32::TRANSPARENT
            })
            .show(ctx, |ui| {
                ui.set_width(width);
                ui.vertical_centered(|ui| {
                    ui.spacing_mut().item_spacing.y = 8.;
                    if meter_visible {
                        card(
                            ui,
                            width,
                            &locale::format(ui, "Saving {name}...", &[("name", &name)]),
                            self.displayed_ratio,
                            None,
                        );
                    }
                    if let Some(task) = &self.task {
                        let title = match task.stage {
                            SaveStage::PrepareLevels => "Saving levels",
                            SaveStage::Backup => "Backup",
                            SaveStage::WriteProject => "Saving main file...",
                            SaveStage::RemoveObsoleteLevels => "Removing obsolete files",
                            SaveStage::ExportPng => "PNG export",
                            SaveStage::ExportSimplified => "Simplified data...",
                            SaveStage::ExportTiled => "Exporting Tiled...",
                        };
                        let label = match (task.stage, task.label.as_deref()) {
                            (SaveStage::Backup, _) if task.done + 1 == task.total => {
                                locale::tr(ui, "Removing older backups").to_owned()
                            }
                            (SaveStage::WriteProject, _) => {
                                locale::tr(ui, "Preparing...").to_owned()
                            }
                            (SaveStage::PrepareLevels, Some(name)) => {
                                locale::format(ui, "Level {name}", &[("name", name)])
                            }
                            (SaveStage::RemoveObsoleteLevels, Some(name)) => {
                                locale::format(ui, "Checking {name}", &[("name", name)])
                            }
                            (_, label) => label.unwrap_or("").to_owned(),
                        };
                        card(
                            ui,
                            width.min(422.),
                            locale::tr(ui, title),
                            task.done as f32 / task.total as f32,
                            Some(&label),
                        );
                    }
                });
            });
    }
}

impl EditorApp {
    pub(super) fn holding_save_completion(&self) -> bool {
        self.saving_progress.as_ref().is_some_and(|progress| {
            let progress = progress.lock().unwrap_or_else(|error| error.into_inner());
            progress.completed_at.is_some() && progress.meter_visible(Instant::now())
        })
    }

    pub(super) fn clear_save_progress(&mut self) {
        self.saving_progress = None;
        self.save_attempt = None;
    }

    pub(super) fn animate_save_progress(&mut self, ctx: &egui::Context, now: Instant) {
        let Some(progress) = &self.saving_progress else {
            return;
        };
        let mut progress = progress.lock().unwrap_or_else(|error| error.into_inner());
        let visible = progress.animate_meter(now);
        let repaint = visible
            && (progress.displayed_ratio != progress.target_ratio()
                || progress.completed_at.is_some());
        drop(progress);
        if repaint {
            ctx.request_repaint_after(FRAME_INTERVAL);
        } else if !visible && self.saving.is_none() {
            self.clear_save_progress();
        }
    }
}

fn card(ui: &mut egui::Ui, width: f32, title: &str, ratio: f32, task_label: Option<&str>) {
    project_dialogs::dialog_frame(8)
        .stroke(Stroke::new(1_f32, theme::LIGHT))
        .corner_radius(0)
        .show(ui, |ui| {
            let width = (width - 18.).max(1.);
            ui.set_width(width);
            ui.spacing_mut().item_spacing.y = 0.;
            let (heading, _) = ui.allocate_exact_size(egui::vec2(width, 30.), Sense::hover());
            let painter = ui
                .painter()
                .with_clip_rect(ui.clip_rect().intersect(heading));
            painter.text(
                heading.left_center() - egui::vec2(0., 3.),
                Align2::LEFT_CENTER,
                title,
                FontId::new(18.667, egui::FontFamily::Name("panel-title".into())),
                theme::ACCENT,
            );
            let (track, _) = ui.allocate_exact_size(egui::vec2(width, 12.), Sense::hover());
            ui.painter().rect(
                track,
                2.,
                theme::DARK,
                Stroke::new(1_f32, theme::ACCENT),
                StrokeKind::Inside,
            );
            let bounds = track.shrink(2.);
            let fill = Rect::from_min_size(
                bounds.min,
                egui::vec2(
                    (bounds.width() * ratio.clamp(0., 1.)).max(1.),
                    bounds.height(),
                ),
            );
            for (blur, color) in [
                (16, Color32::from_rgb(121, 17, 5)),
                (8, Color32::from_rgb(213, 105, 50)),
            ] {
                ui.painter().add(
                    egui::epaint::Shadow {
                        offset: [0, 0],
                        blur,
                        spread: 0,
                        color,
                    }
                    .as_shape(fill, 2.),
                );
            }
            ui.painter().rect_filled(
                fill,
                2.,
                if task_label.is_some() {
                    theme::ACCENT
                } else {
                    Color32::from_rgb(255, 255, 68)
                },
            );
            if let Some(label) = task_label {
                let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 20.), Sense::hover());
                ui.painter().text(
                    rect.right_top() - egui::vec2(1., 0.),
                    Align2::RIGHT_TOP,
                    label,
                    FontId::proportional(12.),
                    Color32::from_rgb(115, 127, 157),
                );
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_154_save_title_keeps_the_completed_meter_above_a_new_export() {
        let now = Instant::now();
        let mut progress = SaveProgress::default();
        progress.record(SaveStage::WriteProject, 1, 1, None, &[]);
        progress.record(SaveStage::ExportPng, 0, 3, None, &[]);
        assert!(progress.animate_meter(now));
        assert_eq!(progress.title_progress(now), Some((9, 9)));
        let handoff = now + COMPLETION_HOLD;
        assert!(!progress.animate_meter(handoff));
        assert_eq!(progress.title_progress(handoff), Some((0, 3)));
    }

    #[test]
    fn preview_154_save_meter_eases_towards_each_stage_and_finishes_faster() {
        let now = Instant::now();
        let mut progress = SaveProgress::default();
        progress.record(SaveStage::Backup, 0, 4, None, &[]);
        assert!(progress.animate_meter(now));
        let first = progress.displayed_ratio;
        assert!((first - 0.3 * (4. / 9.)).abs() < 0.00001);
        progress.animate_meter(now + FRAME_INTERVAL);
        let next = progress.displayed_ratio;
        assert!((next - (first + 0.3 * (4. / 9. - first))).abs() < 0.00001);

        progress.record(SaveStage::WriteProject, 1, 1, None, &[]);
        let completed = now + 2 * FRAME_INTERVAL;
        assert!(progress.animate_meter(completed));
        assert!((progress.displayed_ratio - (next + 0.6 * (1. - next))).abs() < 0.00001);
        for frame in 1..12 {
            assert!(progress.animate_meter(completed + frame * FRAME_INTERVAL));
        }
        assert_eq!(progress.displayed_ratio, 1.);
        assert!(!progress.animate_meter(completed + COMPLETION_HOLD));
        assert_eq!(progress.title_progress(completed + COMPLETION_HOLD), None);
    }

    #[test]
    fn preview_154_successful_save_keeps_the_meter_until_the_hold_expires() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Map.ldtk");
        let mut project = Project::new();
        project.save(&path).unwrap();
        let mut app = EditorApp::empty();
        app.save_attempt = Some(path);
        let progress = Arc::new(Mutex::new(SaveProgress::default()));
        app.saving_progress = Some(progress.clone());
        let (tx, rx) = mpsc::channel();
        app.saving = Some(rx);
        tx.send(Ok(services::SavedProject {
            project,
            report: ldtk_core::SaveReport::default(),
        }))
        .unwrap();

        let ctx = egui::Context::default();
        app.poll_services(&ctx);
        assert!(app.saving.is_none());
        assert!(app.is_busy());
        let completed = progress.lock().unwrap().completed_at.unwrap();
        assert_eq!(
            progress.lock().unwrap().title_progress(completed),
            Some((9, 9))
        );
        app.animate_save_progress(&ctx, completed + COMPLETION_HOLD);
        assert!(!app.is_busy());
        assert!(app.save_attempt.is_none());
    }

    #[test]
    fn preview_154_save_title_tracks_publication_then_the_active_export() {
        let now = Instant::now();
        let mut progress = SaveProgress::default();
        let levels = ["Start".to_owned(), "Town".to_owned()];

        assert_eq!(progress.title_progress(now), Some((0, 9)));
        progress.record(SaveStage::Backup, 0, 4, Some("Map.ldtk"), &levels);
        assert_eq!(progress.title_progress(now), Some((4, 9)));
        progress.record(SaveStage::WriteProject, 0, 1, None, &levels);
        assert_eq!(progress.title_progress(now), Some((6, 9)));
        progress.record(SaveStage::PrepareLevels, 0, 2, Some("Start"), &levels);
        assert_eq!(progress.title_progress(now), Some((7, 9)));
        progress.record(SaveStage::PrepareLevels, 2, 2, None, &levels);
        progress.record(SaveStage::PrepareLevels, 0, 0, None, &levels);
        assert_eq!(progress.task.as_ref().unwrap().done, 2);
        progress.record(
            SaveStage::RemoveObsoleteLevels,
            0,
            2,
            Some("Start.ldtkl"),
            &levels,
        );
        assert_eq!(progress.title_progress(now), Some((8, 9)));
        assert!(progress.export_task().is_none());

        progress.record(SaveStage::WriteProject, 1, 1, None, &levels);
        assert!(progress.animate_meter(now));
        assert_eq!(progress.title_progress(now), Some((9, 9)));
        progress.record(SaveStage::ExportPng, 0, 2, None, &levels);
        let exporting = now + COMPLETION_HOLD;
        assert_eq!(progress.title_progress(exporting), Some((0, 2)));
        assert_eq!(
            progress.export_task().unwrap().label.as_deref(),
            Some("Start")
        );
        progress.record(SaveStage::ExportPng, 1, 2, None, &levels);
        assert_eq!(
            progress.export_task().unwrap().label.as_deref(),
            Some("Town")
        );

        progress.record(SaveStage::ExportPng, 2, 2, None, &levels);
        progress.record(SaveStage::ExportPng, 0, 0, None, &levels);
        assert_eq!(progress.title_progress(exporting), Some((2, 2)));
        assert_eq!(progress.export_task().unwrap().stage, SaveStage::ExportPng);
    }
}
