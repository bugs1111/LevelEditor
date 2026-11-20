use super::*;
use ldtk_core::{BakeMode, Error};
use std::sync::atomic::{AtomicUsize, Ordering};

pub(super) enum LayerBake {
    Choice { uid: i64, intgrid: bool },
    Message(String),
    Running(BakeJob),
}

pub(super) struct BakeJob {
    label: String,
    progress: Arc<AtomicUsize>,
    levels: Vec<String>,
    result: Receiver<ldtk_core::Result<(Project, i64)>>,
}

fn message(error: Error) -> String {
    match error {
        Error::Invalid(message) => message,
        error => error.to_string(),
    }
}

impl EditorApp {
    pub(super) fn begin_layer_bake(&mut self, uid: i64) {
        if self.is_busy() {
            return;
        }
        let Some(project) = self.project.as_ref() else {
            return;
        };
        self.layer_bake = Some(match project.validate_auto_layer_bake(uid) {
            Ok(()) => LayerBake::Choice {
                uid,
                intgrid: project
                    .layer_def(uid)
                    .is_some_and(|d| d["type"] == "IntGrid"),
            },
            Err(error) => LayerBake::Message(message(error)),
        });
    }

    fn run_layer_bake(&mut self, ctx: &egui::Context, uid: i64, mode: BakeMode) {
        if self.is_busy() || !self.settle_deferred_auto_paint() {
            return;
        }
        self.finish_gesture();
        self.finish_stroke();
        let Some(project) = &self.project else {
            return;
        };
        let Some(def) = project.layer_def(uid) else {
            return;
        };
        let label = format!("Baked layer {}", text(def, "identifier", "Layer"));
        let levels = project
            .level_keys()
            .into_iter()
            .map(|key| {
                project
                    .level(key)
                    .map_or("", |level| text(level, "identifier", ""))
                    .to_owned()
            })
            .collect();
        let (send, result) = mpsc::channel();
        let progress = Arc::new(AtomicUsize::new(0));
        let completed = progress.clone();
        let mut project = project.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = project.bake_auto_layer(uid, mode, |done, _| {
                completed.store(done, Ordering::Relaxed);
                ctx.request_repaint();
            });
            let _ = send.send(result.map(|baked| (project, baked)));
            ctx.request_repaint();
        });
        self.layer_bake = Some(LayerBake::Running(BakeJob {
            label,
            progress,
            levels,
            result,
        }));
    }

    pub(super) fn poll_layer_bake(&mut self) {
        let Some(LayerBake::Running(job)) = &self.layer_bake else {
            return;
        };
        let result = match job.result.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => return,
            Err(mpsc::TryRecvError::Disconnected) => {
                Err(Error::Invalid("Layer baking was interrupted.".into()))
            }
        };
        let Some(LayerBake::Running(job)) = self.layer_bake.take() else {
            return;
        };
        match result {
            Ok((project, baked)) => {
                let active = self
                    .selected
                    .and_then(|key| self.project.as_ref()?.layer(key, self.layer))
                    .and_then(|layer| layer["layerDefUid"].as_i64());
                if self
                    .edit_recoverable(&job.label, |p| {
                        *p = project;
                        Ok(())
                    })
                    .is_some()
                {
                    self.definition_uid = Some(baked);
                    if let Some(index) = self.selected.and_then(|key| {
                        let layers =
                            self.project.as_ref()?.level(key)?["layerInstances"].as_array()?;
                        let find =
                            |uid| layers.iter().position(|layer| layer["layerDefUid"] == uid);
                        active.and_then(find).or_else(|| find(baked))
                    }) {
                        self.layer = index;
                        self.normalize_palette();
                    }
                }
            }
            Err(error) => self.layer_bake = Some(LayerBake::Message(message(error))),
        }
    }

    pub(super) fn layer_bake_dialog(&mut self, ctx: &egui::Context) {
        let Some(state) = &self.layer_bake else {
            return;
        };
        let width = (ctx.content_rect().width() - 40.).clamp(180., 360.);
        let running = matches!(state, LayerBake::Running(_));
        let modal = egui::Modal::new(egui::Id::new("bake-auto-layer"))
            .frame(project_dialogs::dialog_frame(20))
            .backdrop_color(Color32::from_black_alpha(if running { 0 } else { 140 }))
            .show(ctx, |ui| {
                ui.set_width(width);
                ui.spacing_mut().item_spacing = egui::vec2(1., 2.);
                let button = |ui: &mut egui::Ui, label: &str, w, gray| {
                    project_dialogs::button(ui, label, egui::vec2(w, 35.), gray, 13.333).clicked()
                };
                match state {
                    LayerBake::Choice { uid, intgrid } => {
                        ui.add_space(14.);
                        ui.label(RichText::new(locale::tr(ui, "'Baking' an auto-layer will flatten it to create a new regular 'Tiles layer'. The copy will contain all the tiles generated from the auto-layer rules.")).size(14.5).color(Color32::WHITE));
                        ui.add_space(12.);
                        ui.label(RichText::new(locale::tr(ui, "What would you like to do with the original layer after baking it?")).size(14.5).color(Color32::WHITE));
                        ui.add_space(12.);
                        let mut mode = None;
                        if button(ui, "Bake, then delete original layer", width, false) {
                            mode = Some(BakeMode::Delete);
                        }
                        if *intgrid && button(ui, "Bake, then empty original layer", width, false) {
                            mode = Some(BakeMode::Empty);
                        }
                        let cancel = ui.horizontal(|ui| {
                            let cancel_width = width * 0.275;
                            if button(ui, "Keep both baked result and original", width - cancel_width - 1., false) {
                                mode = Some(BakeMode::Keep);
                            }
                            button(ui, "Cancel", cancel_width, true)
                        }).inner;
                        (mode.map(|mode| (*uid, mode)), cancel)
                    }
                    LayerBake::Message(text) => {
                        ui.label(RichText::new(text).size(15.).color(Color32::WHITE));
                        ui.add_space(12.);
                        (None, button(ui, "OK", width, false))
                    }
                    LayerBake::Running(job) => {
                        let done = job.progress.load(Ordering::Relaxed);
                        let total = job.levels.len();
                        let fraction = if total == 0 { 1. } else { done as f32 / total as f32 };
                        ui.label(locale::tr(ui, "Baking layer instances"));
                        ui.add(egui::ProgressBar::new(fraction).text(
                            job.levels.get(done).map_or("", String::as_str)
                        ));
                        (None, false)
                    }
                }
            });
        if let Some((uid, mode)) = modal.inner.0 {
            self.layer_bake = None;
            self.run_layer_bake(ctx, uid, mode);
        } else if !running && (modal.inner.1 || modal.should_close()) {
            self.layer_bake = None;
        }
    }
}
