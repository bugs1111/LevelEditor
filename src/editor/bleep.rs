//! Short-lived LevelRender selection/removal feedback, held in the egui context.
use super::*;

#[derive(Clone)]
enum Shape {
    Rect { min: [f32; 2], size: [f32; 2] },
    Circle,
}
#[derive(Clone)]
struct Bleep {
    shape: Shape,
    center: [f32; 2],
    color: Color32,
    thickness: f32,
    born: f64,
    speed: f32,
    extra_scale: f32,
    delay: f64,
    repeats: u32,
}
impl Bleep {
    fn sample(&self, now: f64) -> Option<(f32, f32)> {
        let elapsed = (now - self.born).max(0.);
        if elapsed < self.delay {
            // Source adds the graphic immediately, before its delay counter runs.
            return Some((1., 1.));
        }
        let elapsed = elapsed - self.delay;
        let duration = 1. / (0.064 * 60. * f64::from(self.speed));
        let cycle = duration + 0.03;
        let index = (elapsed / cycle).floor() as u32;
        if index >= self.repeats {
            return None;
        }
        let phase = elapsed - f64::from(index) * cycle;
        if phase >= duration {
            return (index + 1 < self.repeats).then_some((1., 0.));
        }
        let ratio = (phase / duration) as f32;
        Some((
            (self.extra_scale + 1.6) - (self.extra_scale + 0.6) * ratio,
            1. - ratio,
        ))
    }
}
#[derive(Clone)]
struct ReferenceHover {
    key: LevelKey,
    iid: String,
    frame: u64,
    widget: egui::Id,
}
#[derive(Clone, Default)]
struct Queue {
    project: String,
    level: String,
    items: Vec<Bleep>,
}
fn id() -> egui::Id {
    egui::Id::new("level-render-bleeps")
}
fn round(v: f32) -> f32 {
    (v + 0.5).floor()
}
fn rect_bleep(x: f32, y: f32, w: f32, h: f32, color: Color32, thickness: f32) -> Bleep {
    Bleep {
        shape: Shape::Rect {
            min: [(-5. - w * 0.5).trunc(), (-5. - h * 0.5).trunc()],
            size: [w + 10., h + 10.],
        },
        center: [(x + w * 0.5).trunc(), (y + h * 0.5).trunc()],
        color,
        thickness,
        born: 0.,
        speed: 1.,
        extra_scale: 0.,
        delay: 0.,
        repeats: 1,
    }
}
fn layer_geometry(
    project: &Project,
    key: LevelKey,
    layer: usize,
    view: &crate::viewport::Viewport,
) -> ([f32; 2], f32) {
    let origin = canvas::layer_to_world(project, key, layer, view, [0., 0.]);
    let unit = canvas::layer_to_world(project, key, layer, view, [1., 0.]);
    let scale = unit[0] - origin[0];
    let li = project.layer(key, layer).unwrap();
    // Editing coordinates scale the layer offset. LevelRender's pxParallax adds it raw.
    let origin = std::array::from_fn(|axis| {
        round(
            origin[axis]
                + number(
                    li,
                    if axis == 0 {
                        "__pxTotalOffsetX"
                    } else {
                        "__pxTotalOffsetY"
                    },
                    0.,
                ) * (1. - scale),
        )
    });
    (origin, scale)
}
fn entity_bleep(
    project: &Project,
    key: LevelKey,
    layer: usize,
    view: &crate::viewport::Viewport,
    entity: &Value,
) -> Option<Bleep> {
    let def = project.json["defs"]["entities"]
        .as_array()?
        .iter()
        .find(|d| d["uid"] == entity["defUid"])?;
    let (offset, scale) = layer_geometry(project, key, layer, view);
    let size = [number(entity, "width", 16.), number(entity, "height", 16.)];
    let top: [f32; 2] = std::array::from_fn(|axis| {
        let pos = entity["px"][axis].as_f64().unwrap_or(0.) as f32;
        let pivot = number(def, if axis == 0 { "pivotX" } else { "pivotY" }, 0.);
        ((pos - size[axis] * pivot) * scale).trunc() + offset[axis]
    });
    Some(rect_bleep(
        top[0],
        top[1],
        size[0] * scale,
        size[1] * scale,
        canvas::entity_smart_color(project, entity, def, true),
        2.,
    ))
}
fn identity(project: &Project, key: LevelKey) -> Option<(String, String)> {
    Some((
        project.json["iid"].as_str()?.to_owned(),
        project.level(key)?["iid"].as_str()?.to_owned(),
    ))
}
fn enqueue(ctx: &egui::Context, project: &Project, key: LevelKey, mut bleep: Bleep) {
    let Some((project, level)) = identity(project, key) else {
        return;
    };
    bleep.born = ctx.input(|i| i.time);
    ctx.data_mut(|data| {
        let mut queue = data.get_temp::<Queue>(id()).unwrap_or_default();
        if queue.project != project || queue.level != level {
            queue = Queue {
                project,
                level,
                items: Vec::new(),
            };
        }
        queue.items.push(bleep);
        data.insert_temp(id(), queue);
    });
    ctx.request_repaint();
}
#[derive(Clone, Default)]
struct ErrorScan {
    layer: usize,
    entity: usize,
    candidate: Option<EntityError>,
    result: Option<EntityError>,
    running: bool,
    next_scan: f64,
    completed_frame: Option<u64>,
    last_bleep_frame: Option<u64>,
}
#[derive(Clone)]
struct EntityError {
    layer_uid: i64,
    iid: String,
    name: String,
    kind: EntityErrorKind,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EntityErrorKind {
    Tag,
    Field,
}
fn entity_error_kind(
    project: &Project,
    key: LevelKey,
    layer: &Value,
    entity: &Value,
    def: &Value,
) -> Option<EntityErrorKind> {
    if layer["layerDefUid"]
        .as_i64()
        .and_then(|uid| project.layer_def(uid))
        .is_some_and(|layer_def| !library::entity_allowed(def, layer_def))
    {
        return Some(EntityErrorKind::Tag);
    }
    let iid = entity["iid"].as_str()?;
    entity["fieldInstances"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|field| field["defUid"].as_i64())
        .any(|uid| {
            project
                .field_value_errors(key, Some(iid), uid)
                .is_ok_and(|errors| !errors.is_empty())
        })
        .then_some(EntityErrorKind::Field)
}
fn entity_error(
    project: &Project,
    key: LevelKey,
    layer: &Value,
    entity: &Value,
) -> Option<EntityError> {
    let def = project.json["defs"]["entities"]
        .as_array()?
        .iter()
        .find(|def| def["uid"] == entity["defUid"])?;
    Some(EntityError {
        kind: entity_error_kind(project, key, layer, entity, def)?,
        layer_uid: layer["layerDefUid"].as_i64()?,
        iid: entity["iid"].as_str()?.into(),
        name: def["identifier"].as_str()?.into(),
    })
}
// The source's cached first error checks all layers, including hidden/offscreen ones.
// Yield between entities when the soft 1 ms budget is exhausted. A single diagnostic
// (including filesystem/reference lookup) and final candidate revalidation may exceed it.
fn advance_error_scan(
    project: &Project,
    key: LevelKey,
    scan: &mut ErrorScan,
    budget: std::time::Duration,
) -> bool {
    let started = Instant::now();
    let Some(layers) = project
        .level(key)
        .and_then(|level| level["layerInstances"].as_array())
    else {
        return true;
    };
    let mut exhausted = false;
    while scan.layer < layers.len() {
        let layer = &layers[scan.layer];
        let entities = layer["entityInstances"]
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or_default();
        let Some(entity) = entities.get(scan.entity) else {
            scan.layer += 1;
            scan.entity = 0;
            continue;
        };
        // A completed scan can publish without taking another frame.
        if exhausted {
            return false;
        }
        if let Some(error) = entity_error(project, key, layer, entity) {
            scan.candidate = Some(error);
            // Level.getFirstError breaks only the inner loop: each later error layer
            // replaces the previous candidate, with its first erroneous entity.
            scan.layer += 1;
            scan.entity = 0;
        } else {
            scan.entity += 1;
        }
        exhausted = started.elapsed() >= budget;
    }
    true
}

fn error_scan_id(project: &Project, key: LevelKey) -> Option<egui::Id> {
    Some(id().with("errors").with(identity(project, key)?))
}

pub(super) fn reset_error_scans(ctx: &egui::Context) {
    ctx.data_mut(|data| data.remove_by_type::<ErrorScan>());
}

fn update_level_errors(ctx: &egui::Context, project: &Project, key: LevelKey) -> Option<egui::Id> {
    let state_id = error_scan_id(project, key)?;
    let mut scan = ctx
        .data(|data| data.get_temp::<ErrorScan>(state_id))
        .unwrap_or_default();
    let now = ctx.input(|input| input.time);
    if !scan.running && now < scan.next_scan {
        ctx.request_repaint_after(std::time::Duration::from_secs_f64(scan.next_scan - now));
        return Some(state_id);
    }
    let frame = ctx.cumulative_frame_nr();
    let budget_id = id().with("error-scan-budget");
    let spent = ctx
        .data(|data| data.get_temp::<(u64, std::time::Duration)>(budget_id))
        .filter(|(from, _)| *from == frame)
        .map_or(std::time::Duration::ZERO, |(_, spent)| spent);
    let remaining = std::time::Duration::from_millis(1).saturating_sub(spent);
    if remaining.is_zero() {
        ctx.request_repaint();
        return Some(state_id);
    }
    if !scan.running {
        scan.layer = 0;
        scan.entity = 0;
        scan.candidate = None;
        scan.running = true;
    }
    let started = Instant::now();
    if advance_error_scan(project, key, &mut scan, remaining) {
        scan.running = false;
        scan.completed_frame = Some(frame);
        scan.result = scan.candidate.take();
        scan.next_scan = now + 0.7;
        ctx.request_repaint_after(std::time::Duration::from_secs_f64(0.7));
    } else {
        ctx.request_repaint();
    }
    ctx.data_mut(|data| {
        data.insert_temp(state_id, scan);
        data.insert_temp(budget_id, (frame, spent + started.elapsed()));
    });
    Some(state_id)
}

pub(crate) fn level_entity_error(
    ctx: &egui::Context,
    project: &Project,
    key: LevelKey,
) -> Option<String> {
    let state_id = update_level_errors(ctx, project, key)?;
    let error = ctx
        .data(|data| data.get_temp::<ErrorScan>(state_id))?
        .result?;
    Some(match error.kind {
        EntityErrorKind::Tag => format!("Incorrect tag: {}", error.name),
        EntityErrorKind::Field => format!("Invalid field value: {}", error.name),
    })
}
// Source group moves mark source grid cells and resulting elements; Points have no bounds.
pub(super) fn mark_group(
    history: &mut History,
    project: &Project,
    key: LevelKey,
    group: &Value,
    entities: bool,
) {
    mark_group_at(history, project, key, group, entities, None);
}
pub(super) fn mark_moved_group(
    history: &mut History,
    project: &Project,
    key: LevelKey,
    group: &Value,
    delta: [f64; 2],
) {
    mark_group_at(history, project, key, group, true, Some(delta));
}
fn mark_group_at(
    history: &mut History,
    project: &Project,
    key: LevelKey,
    group: &Value,
    entities: bool,
    delta: Option<[f64; 2]>,
) {
    for entry in group["layers"].as_array().into_iter().flatten() {
        let Some(index) = project
            .level(key)
            .and_then(|l| l["layerInstances"].as_array())
            .and_then(|ls| {
                ls.iter()
                    .position(|l| l["layerDefUid"] == entry["layerDefUid"])
            })
        else {
            continue;
        };
        let Some(layer) = project.layer(key, index) else {
            continue;
        };
        let grid = number(layer, "__gridSize", 16.) as i32;
        let raw_delta = delta
            .map(|delta| {
                if layer["__type"] == "Entities" {
                    delta.map(|d| d.trunc() as i32)
                } else {
                    let smart = number(group, "scaledGridSize", grid as f32) as f64;
                    let scaled = grid as f64 * number(&entry["geometry"], "scale", 1.) as f64;
                    delta.map(|d| {
                        (d / smart).trunc() as i32 * (smart / scaled).trunc() as i32 * grid
                    })
                }
            })
            .unwrap_or([0, 0]);
        if entities {
            for entity in entry["clipboard"]["entityInstances"]
                .as_array()
                .into_iter()
                .flatten()
            {
                if let Some(current) = layer["entityInstances"]
                    .as_array()
                    .and_then(|es| es.iter().find(|e| e["iid"] == entity["iid"]))
                {
                    if delta.is_some() {
                        let mut moved = current.clone();
                        moved["px"] =
                            serde_json::json!(std::array::from_fn::<_, 2, _>(|axis| current["px"]
                                [axis]
                                .as_i64()
                                .unwrap_or(0)
                                + i64::from(raw_delta[axis])));
                        history.mark_entity_change(project, key, index, &moved);
                    } else {
                        history.mark_entity_change(project, key, index, current);
                    }
                }
            }
        }
        let origin = [
            entry["rect"][0].as_i64().unwrap_or(0) as i32 + raw_delta[0] / grid,
            entry["rect"][1].as_i64().unwrap_or(0) as i32 + raw_delta[1] / grid,
        ];
        let width = entry["rect"][2].as_i64().unwrap_or(1).max(1) as i32;
        for cell in entry["cellMask"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_i64)
        {
            history.mark_grid_change(
                project,
                key,
                index,
                origin[0] + cell as i32 % width,
                origin[1] + cell as i32 / width,
            );
        }
        let mut seen = std::collections::BTreeSet::new();
        for tile in entry["clipboard"]["gridTiles"]
            .as_array()
            .into_iter()
            .flatten()
        {
            let cell = [
                origin[0] + tile["px"][0].as_i64().unwrap_or(0) as i32 / grid,
                origin[1] + tile["px"][1].as_i64().unwrap_or(0) as i32 / grid,
            ];
            if seen.insert(cell) {
                history.mark_grid_change(project, key, index, cell[0], cell[1]);
            }
        }
    }
}
impl EditorApp {
    fn update_error_bleep(&self, ctx: &egui::Context) {
        if self.world {
            return;
        }
        let (Some(project), Some(key)) = (&self.project, self.selected) else {
            return;
        };
        let Some(state_id) = update_level_errors(ctx, project, key) else {
            return;
        };
        let frame = ctx.cumulative_frame_nr();
        let Some(error) = ctx.data_mut(|data| {
            let mut scan = data.get_temp::<ErrorScan>(state_id)?;
            if scan.completed_frame != Some(frame) || scan.last_bleep_frame == Some(frame) {
                return None;
            }
            let error = scan.result.clone()?;
            scan.last_bleep_frame = Some(frame);
            data.insert_temp(state_id, scan);
            Some(error)
        }) else {
            return;
        };
        let owner = project
            .level(key)
            .and_then(|level| level["layerInstances"].as_array())
            .and_then(|layers| {
                layers
                    .iter()
                    .enumerate()
                    .find(|(_, li)| li["layerDefUid"] == error.layer_uid)
            });
        if let Some((layer_index, layer)) = owner
            && let Some(entity) = layer["entityInstances"]
                .as_array()
                .and_then(|entities| entities.iter().find(|e| e["iid"] == error.iid))
            && entity_error(project, key, layer, entity).is_some()
        {
            let inspected = self.panel == Some(Panel::Entity)
                && self.layer == layer_index
                && self
                    .entity_index
                    .is_some_and(|index| layer["entityInstances"][index]["iid"] == error.iid);
            if !inspected
                && let Some(mut bleep) =
                    entity_bleep(project, key, layer_index, &self.viewport, entity)
            {
                bleep.color = Color32::RED;
                bleep.speed = 0.4;
                bleep.extra_scale = 1.;
                enqueue(ctx, project, key, bleep);
            }
        }
    }
    pub(super) fn bleep_history(&self, key: LevelKey, bounds: [f32; 4], redo: bool) {
        let (Some(ctx), Some(project)) = (&self.repaint, &self.project) else {
            return;
        };
        let mut b = rect_bleep(
            bounds[0],
            bounds[1],
            bounds[2],
            bounds[3],
            if redo { Color32::GREEN } else { Color32::RED },
            1.,
        );
        if !redo {
            b.speed = 0.75;
        }
        enqueue(ctx, project, key, b);
    }
    pub(super) fn bleep_entity(&self, key: LevelKey, layer: usize, entity: &Value) {
        let (Some(ctx), Some(project)) = (&self.repaint, &self.project) else {
            return;
        };
        if let Some(bleep) = entity_bleep(project, key, layer, &self.viewport, entity) {
            enqueue(ctx, project, key, bleep);
        }
    }
    pub(super) fn bleep_entity_effect(
        &self,
        key: LevelKey,
        layer: usize,
        entity: &Value,
        color: Option<Color32>,
        delay: f64,
        repeats: u32,
    ) {
        let (Some(ctx), Some(project)) = (&self.repaint, &self.project) else {
            return;
        };
        if let Some(mut b) = entity_bleep(project, key, layer, &self.viewport, entity) {
            if let Some(color) = color {
                b.color = color;
            }
            b.delay = delay;
            b.repeats = repeats;
            enqueue(ctx, project, key, b);
        }
    }
    pub(super) fn hover_entity_reference(&self, response: &egui::Response, iid: &str) {
        let ctx = response.ctx.clone();
        let state_id = response.id.with("reference-hover");
        let hovered = response.hovered() && !self.picking_field();
        let previous = ctx.data(|d| d.get_temp::<bool>(state_id)).unwrap_or(false);
        ctx.data_mut(|d| d.insert_temp(state_id, hovered));
        let (Some(project), Some(key)) = (&self.project, self.selected) else {
            return;
        };
        let target = project
            .level(key)
            .and_then(|l| l["layerInstances"].as_array())
            .and_then(|layers| {
                layers.iter().enumerate().find_map(|(index, l)| {
                    l["entityInstances"]
                        .as_array()?
                        .iter()
                        .find(|e| e["iid"] == iid)
                        .map(|e| (index, e))
                })
            });
        if hovered && let Some((layer, entity)) = target {
            let frame = ctx.cumulative_frame_nr();
            ctx.data_mut(|d| {
                d.insert_temp(
                    id().with("ref-hover"),
                    ReferenceHover {
                        key,
                        iid: iid.into(),
                        frame,
                        widget: response.id,
                    },
                )
            });
            if !previous {
                self.bleep_entity_effect(key, layer, entity, Some(Color32::MAGENTA), 0., 2);
            }
            ctx.request_repaint();
        } else if ctx
            .data(|d| d.get_temp::<ReferenceHover>(id().with("ref-hover")))
            .is_some_and(|h| h.widget == response.id)
        {
            ctx.data_mut(|d| d.remove::<ReferenceHover>(id().with("ref-hover")));
        }
    }
    fn draw_reference_hover(&self, ui: &egui::Ui, rect: Rect) {
        let Some(hover) = ui
            .ctx()
            .data(|d| d.get_temp::<ReferenceHover>(id().with("ref-hover")))
        else {
            return;
        };
        if self.world
            || self.picking_field()
            || self.selected != Some(hover.key)
            || ui.ctx().cumulative_frame_nr() > hover.frame + 1
        {
            return;
        }
        let Some(project) = &self.project else {
            return;
        };
        let Some(e) = project
            .level(hover.key)
            .and_then(|l| l["layerInstances"].as_array())
            .into_iter()
            .flatten()
            .flat_map(|l| l["entityInstances"].as_array().into_iter().flatten())
            .find(|e| e["iid"] == hover.iid)
        else {
            return;
        };
        let center = std::array::from_fn(|axis| {
            round(
                e["px"][axis].as_f64().unwrap_or(0.) as f32
                    + (0.5 - e["__pivot"][axis].as_f64().unwrap_or(0.) as f32)
                        * number(e, if axis == 0 { "width" } else { "height" }, 16.),
            )
        });
        ui.painter().with_clip_rect(rect).circle_stroke(
            egui::Pos2::from(
                self.viewport
                    .to_screen(center, rect.min.into(), rect.size().into()),
            ),
            (number(e, "width", 16.).max(number(e, "height", 16.)) * 0.5 + 8.) * self.viewport.zoom,
            Stroke::new(2. * self.viewport.zoom, Color32::MAGENTA),
        );
    }
    pub(super) fn bleep_point(&self, key: LevelKey, position: [f32; 2], color: Color32) {
        let (Some(ctx), Some(project)) = (&self.repaint, &self.project) else {
            return;
        };
        enqueue(
            ctx,
            project,
            key,
            Bleep {
                shape: Shape::Circle,
                center: position.map(round),
                color,
                thickness: 2.,
                born: 0.,
                speed: 1.,
                extra_scale: 0.,
                delay: 0.,
                repeats: 1,
            },
        );
    }
    pub(super) fn bleep_layer_cells(
        &self,
        key: LevelKey,
        layer: usize,
        region: [i32; 4],
        color: Color32,
    ) {
        let (Some(ctx), Some(project)) = (&self.repaint, &self.project) else {
            return;
        };
        let Some(li) = project.layer(key, layer) else {
            return;
        };
        let (offset, scale) = layer_geometry(project, key, layer, &self.viewport);
        let grid = number(li, "__gridSize", 16.) * scale;
        enqueue(
            ctx,
            project,
            key,
            rect_bleep(
                region[0] as f32 * grid + offset[0],
                region[1] as f32 * grid + offset[1],
                region[2] as f32 * grid,
                region[3] as f32 * grid,
                color,
                1.,
            ),
        );
    }
    pub(super) fn bleep_selected_element(
        &self,
        key: LevelKey,
        layer: usize,
        cell: [i32; 2],
        element: selection_click::Element,
    ) {
        let Some(project) = &self.project else {
            return;
        };
        let Some(li) = project.layer(key, layer) else {
            return;
        };
        match element {
            selection_click::Element::Entity(index) => {
                self.bleep_entity(key, layer, &li["entityInstances"][index])
            }
            selection_click::Element::Point { entity, .. } => {
                let e = &li["entityInstances"][entity];
                let def = project.json["defs"]["entities"]
                    .as_array()
                    .and_then(|defs| defs.iter().find(|d| d["uid"] == e["defUid"]));
                if let Some(def) = def {
                    self.bleep_layer_cells(
                        key,
                        layer,
                        [cell[0], cell[1], 1, 1],
                        theme::color(text(def, "color", "#ffcc00")),
                    );
                }
            }
            selection_click::Element::Grid => {
                let color = if li["__type"] == "IntGrid" {
                    let width = number(li, "__cWid", 1.) as i32;
                    let value = &li["intGridCsv"][(cell[0] + cell[1] * width) as usize];
                    let color = li["layerDefUid"]
                        .as_i64()
                        .and_then(|uid| project.layer_def(uid))
                        .and_then(|d| d["intGridValues"].as_array())
                        .and_then(|values| values.iter().find(|v| v["value"] == *value))
                        .and_then(|v| v["color"].as_str());
                    theme::color(color.unwrap_or("#ffcc00"))
                } else {
                    theme::ACCENT
                };
                self.bleep_layer_cells(key, layer, [cell[0], cell[1], 1, 1], color);
            }
        }
    }
    pub(super) fn draw_bleeps(&self, ui: &egui::Ui, rect: Rect) {
        self.draw_reference_hover(ui, rect);
        let ctx = ui.ctx();
        self.update_error_bleep(ctx);
        let Some(mut queue) = ctx.data(|data| data.get_temp::<Queue>(id())) else {
            return;
        };
        let current = self
            .project
            .as_ref()
            .and_then(|p| identity(p, self.selected?));
        if current
            .as_ref()
            .is_none_or(|(project, level)| *project != queue.project || *level != queue.level)
        {
            ctx.data_mut(|data| data.remove::<Queue>(id()));
            return;
        }
        let now = ctx.input(|i| i.time);
        queue.items.retain(|b| b.sample(now).is_some());
        if !self.world {
            let painter = ui.painter().with_clip_rect(rect);
            let screen = |point| {
                egui::Pos2::from(self.viewport.to_screen(
                    point,
                    rect.min.into(),
                    rect.size().into(),
                ))
            };
            for b in &queue.items {
                let (scale, alpha) = b.sample(now).expect("retained animation");
                let stroke = Stroke::new(
                    b.thickness * scale * self.viewport.zoom,
                    b.color.gamma_multiply(alpha),
                );
                match b.shape {
                    Shape::Circle => {
                        painter.circle_stroke(
                            screen(b.center),
                            16. * scale * self.viewport.zoom,
                            stroke,
                        );
                    }
                    Shape::Rect { min, size } => {
                        let min =
                            screen([b.center[0] + min[0] * scale, b.center[1] + min[1] * scale]);
                        painter.rect_stroke(
                            Rect::from_min_size(
                                min,
                                egui::vec2(size[0], size[1]) * scale * self.viewport.zoom,
                            ),
                            0.,
                            stroke,
                            egui::StrokeKind::Middle,
                        );
                    }
                }
            }
        }
        if queue.items.is_empty() {
            ctx.data_mut(|data| data.remove::<Queue>(id()));
        } else {
            ctx.data_mut(|data| data.insert_temp(id(), queue));
            ctx.request_repaint();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    const KEY: LevelKey = LevelKey {
        world: None,
        level: 0,
    };
    fn fixture() -> (EditorApp, egui::Context) {
        let mut p = Project::new();
        p.add_layer("Actors", "Entities", 16).unwrap();
        let uid = p.add_entity_def("Actor", 16, 16).unwrap();
        p.update_entity_definition(uid, &json!({"pivotX":0.5,"pivotY":1}))
            .unwrap();
        p.add_entity(KEY, 0, uid, 24, 32).unwrap();
        let mut app = EditorApp::empty();
        app.set_project(p);
        app.fit = false;
        app.viewport.center = [0., 0.];
        app.viewport.zoom = 1.;
        let ctx = egui::Context::default();
        app.repaint = Some(ctx.clone());
        let _ = ctx.run(
            egui::RawInput {
                time: Some(0.),
                ..Default::default()
            },
            |_| {},
        );
        (app, ctx)
    }
    fn draw(app: &EditorApp, ctx: &egui::Context, time: f64) -> Vec<egui::Shape> {
        let rect = Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(400., 400.));
        ctx.run(
            egui::RawInput {
                screen_rect: Some(rect),
                time: Some(time),
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default()
                    .frame(egui::Frame::NONE)
                    .show(ctx, |ui| app.draw_bleeps(ui, rect));
            },
        )
        .shapes
        .into_iter()
        .map(|s| s.shape)
        .filter(|s| match s {
            egui::Shape::Circle(c) => c.stroke.width > 0.,
            egui::Shape::Rect(r) => r.stroke.width > 0.,
            _ => false,
        })
        .collect()
    }
    #[test]
    fn moving_a_group_out_of_bounds_keeps_feedback_at_the_removed_entity_destination() {
        let (mut app, _) = fixture();
        app.select_all();
        app.gesture = Some(tools::Gesture::SelectionMove {
            region: [0, 0, 16, 16],
            press: egui::Pos2::ZERO,
            start: [0, 0],
            end: [25, 0],
            copy: false,
            world_start: [0., 0.],
            world_end: [400., 0.],
            moved: true,
        });
        app.finish_gesture();
        let project = app.project.as_ref().unwrap();
        assert_eq!(
            project.layer(KEY, 0).unwrap()["entityInstances"]
                .as_array()
                .unwrap()
                .len(),
            0
        );
        assert_eq!(
            app.history.level_change_bounds(project, KEY, false),
            Some([416., 16., 16., 16.])
        );
        app.undo_action(false);
        assert_eq!(
            app.project.as_ref().unwrap().layer(KEY, 0).unwrap()["entityInstances"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
    }
    #[test]
    fn reference_hover_flashes_twice_on_entry_and_keeps_a_circle_until_leave() {
        let (app, ctx) = fixture();
        let iid = app.project.as_ref().unwrap().layer(KEY, 0).unwrap()["entityInstances"][0]["iid"]
            .as_str()
            .unwrap();
        let hover_frame = |events| {
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(400., 400.),
                    )),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default()
                        .frame(egui::Frame::NONE)
                        .show(ctx, |ui| {
                            let response = ui.interact(
                                Rect::from_min_max(egui::pos2(10., 10.), egui::pos2(90., 90.)),
                                egui::Id::new("ref-widget"),
                                Sense::hover(),
                            );
                            app.hover_entity_reference(&response, iid);
                            let other = ui.interact(
                                Rect::from_min_max(egui::pos2(200., 10.), egui::pos2(290., 90.)),
                                egui::Id::new("another-reference-to-same-entity"),
                                Sense::hover(),
                            );
                            app.hover_entity_reference(&other, iid);
                        });
                },
            );
        };
        hover_frame(vec![]);
        hover_frame(vec![egui::Event::PointerMoved(egui::pos2(50., 50.))]);
        let queue = ctx.data(|d| d.get_temp::<Queue>(id())).unwrap();
        assert_eq!(queue.items.len(), 1);
        assert_eq!(queue.items[0].color, Color32::MAGENTA);
        assert_eq!(queue.items[0].repeats, 2);
        assert!(
            ctx.data(|d| d.get_temp::<ReferenceHover>(id().with("ref-hover")))
                .is_some(),
            "another nonhovered reference must not remove the current circle"
        );
        hover_frame(vec![]);
        assert_eq!(
            ctx.data(|d| d.get_temp::<Queue>(id())).unwrap().items.len(),
            1
        );
        let shapes = draw(&app, &ctx, ctx.input(|input| input.time));
        assert!(shapes.iter().any(|s|matches!(s,egui::Shape::Circle(c) if c.stroke.color==Color32::MAGENTA && c.radius==16.)));
        hover_frame(vec![egui::Event::PointerGone]);
        assert!(
            ctx.data(|d| d.get_temp::<ReferenceHover>(id().with("ref-hover")))
                .is_none()
        );
    }
    #[test]
    fn undo_and_redo_show_the_committed_bounds_with_distinct_source_colors_and_speed() {
        let (mut app, ctx) = fixture();
        let p = app.project.as_mut().unwrap();
        app.history.begin(p, KEY);
        app.history.mark_rect_change([20., 30., 16., 16.]);
        p.layer_mut(KEY, 0).unwrap()["entityInstances"][0]["px"] = json!([40, 48]);
        assert!(app.history.commit(p));
        app.undo_action(false);
        let queued = ctx.data(|d| d.get_temp::<Queue>(id())).unwrap();
        assert_eq!(queued.items.len(), 1);
        assert_eq!(queued.items[0].color, Color32::RED);
        assert_eq!(queued.items[0].speed, 0.75);
        assert_eq!(queued.items[0].center, [28., 38.]);
        ctx.data_mut(|d| d.remove::<Queue>(id()));
        app.undo_action(true);
        let queued = ctx.data(|d| d.get_temp::<Queue>(id())).unwrap();
        assert_eq!(queued.items.len(), 1);
        assert_eq!(queued.items[0].color, Color32::GREEN);
        assert_eq!(queued.items[0].speed, 1.);
        assert_eq!(queued.items[0].center, [28., 38.]);
    }
    #[test]
    fn delayed_search_flash_is_visible_before_delay_and_repeats_five_times() {
        let (mut app, ctx) = fixture();
        let iid = app.project.as_ref().unwrap().layer(KEY, 0).unwrap()["entityInstances"][0]["iid"]
            .as_str()
            .unwrap()
            .to_owned();
        app.select_search_destination(search::Destination::Entity(iid));
        assert!(!app.fit && app.camera_motion.is_active());
        assert_ne!(app.panel, Some(Panel::Entity));
        let first = draw(&app, &ctx, 0.1);
        assert_eq!(first.len(), 1, "search must not also invoke follow bleeps");
        let egui::Shape::Rect(r) = &first[0] else {
            panic!()
        };
        assert!(
            (r.rect.width() - 26.).abs() < 0.001,
            "delay keeps initial scale one"
        );
        let duration = 1. / (0.064 * 60.);
        let gap = draw(&app, &ctx, 0.2 + duration + 0.01);
        assert!(gap.is_empty() || matches!(&gap[0], egui::Shape::Rect(r) if r.stroke.color.a()==0));
        assert!(!draw(&app, &ctx, 0.2 + 4. * (duration + 0.03) + 0.01).is_empty());
        assert!(draw(&app, &ctx, 0.2 + 5. * duration + 4. * 0.03 + 0.01).is_empty());
    }
    #[test]
    fn follow_entity_has_source_follow_and_single_selection_feedback() {
        let (mut app, ctx) = fixture();
        let iid = app.project.as_ref().unwrap().layer(KEY, 0).unwrap()["entityInstances"][0]["iid"]
            .as_str()
            .unwrap()
            .to_owned();
        assert!(app.follow_entity(&iid));
        assert_eq!(draw(&app, &ctx, 0.).len(), 2);
        assert!(app.selection_group.is_some());
    }
    #[test]
    fn point_draw_rounds_origin_starts_at_source_scale_fades_then_expires() {
        let (app, ctx) = fixture();
        app.bleep_point(KEY, [10.6, -2.5], Color32::RED);
        let shapes = draw(&app, &ctx, 0.);
        let egui::Shape::Circle(circle) = &shapes[0] else {
            panic!("circle")
        };
        assert_eq!(circle.center, egui::pos2(211., 198.));
        assert!((circle.radius - 25.6).abs() < 0.001);
        assert_eq!(circle.stroke.color.a(), 255);
        let shapes = draw(&app, &ctx, 0.5 / (0.064 * 60.));
        let egui::Shape::Circle(circle) = &shapes[0] else {
            panic!()
        };
        assert!((circle.radius - 20.8).abs() < 0.001);
        assert!((circle.stroke.color.a() as i32 - 128).abs() <= 1);
        assert!(draw(&app, &ctx, 1. / (0.064 * 60.) + 0.001).is_empty());
    }
    #[test]
    fn entity_and_cell_bleeps_use_scaled_truncated_body_and_rounded_parallax_origin() {
        let (mut app, ctx) = fixture();
        let p = app.project.as_mut().unwrap();
        p.json["defs"]["layers"][0]["parallaxFactorX"] = json!(0.5);
        p.json["defs"]["layers"][0]["parallaxFactorY"] = json!(0.5);
        p.json["defs"]["layers"][0]["parallaxScaling"] = json!(true);
        p.layer_mut(KEY, 0).unwrap()["__pxTotalOffsetX"] = json!(3);
        p.layer_mut(KEY, 0).unwrap()["__pxTotalOffsetY"] = json!(5);
        app.viewport.center = [101., 61.];
        let entity =
            app.project.as_ref().unwrap().layer(KEY, 0).unwrap()["entityInstances"][0].clone();
        app.bleep_entity(KEY, 0, &entity);
        let shapes = draw(&app, &ctx, 0.);
        assert_eq!(shapes.len(), 1);
        let egui::Shape::Rect(body) = &shapes[0] else {
            panic!()
        };
        let pivot = [
            entity["__pivot"][0].as_f64().unwrap() as f32,
            entity["__pivot"][1].as_f64().unwrap() as f32,
        ];
        let origin = [54., 36.];
        let top = [(24. - 16. * pivot[0]) * 0.5, (32. - 16. * pivot[1]) * 0.5].map(f32::trunc);
        let center = [
            (origin[0] + top[0] + 4.).trunc(),
            (origin[1] + top[1] + 4.).trunc(),
        ];
        let min = app.viewport.to_screen(
            [center[0] - 9. * 1.6, center[1] - 9. * 1.6],
            [0., 0.],
            [400., 400.],
        );
        assert!(
            (body.rect.min.x - min[0]).abs() < 0.001,
            "actual {:?} expected {:?} pivot {:?} entity {:?}",
            body.rect,
            min,
            pivot,
            entity
        );
        assert!((body.rect.width() - 28.8).abs() < 0.001);
        app.bleep_layer_cells(KEY, 0, [2, 3, 1, 2], Color32::YELLOW);
        assert_eq!(draw(&app, &ctx, 0.).len(), 2);
    }
    #[test]
    fn changing_level_or_project_drops_old_overlay_without_mutating_document() {
        let (mut app, ctx) = fixture();
        let before = app.project.as_ref().unwrap().json.clone();
        app.bleep_point(KEY, [0., 0.], Color32::RED);
        assert_eq!(app.project.as_ref().unwrap().json, before);
        let next = app
            .project
            .as_mut()
            .unwrap()
            .add_level(None, "Other", 256, 256)
            .unwrap();
        app.navigate_level(next, false);
        assert!(draw(&app, &ctx, 0.1).is_empty());
    }
    #[test]
    fn single_pick_bleeps_but_deleting_selection_does_not_add_source_unspecified_feedback() {
        let (mut app, ctx) = fixture();
        assert!(app.select_element_at([24., 24.], false, None));
        assert_eq!(draw(&app, &ctx, 0.).len(), 1);
        assert!(draw(&app, &ctx, 1.).is_empty());
        app.delete_selection();
        assert!(draw(&app, &ctx, 1.1).is_empty());
        assert!(
            app.project.as_ref().unwrap().layer(KEY, 0).unwrap()["entityInstances"]
                .as_array()
                .unwrap()
                .is_empty()
        );
    }
    #[test]
    fn point_erase_bleeps_selected_owner_and_unoffset_point_circle() {
        let (mut app, ctx) = fixture();
        let p = app.project.as_mut().unwrap();
        let uid = p.layer(KEY, 0).unwrap()["entityInstances"][0]["defUid"]
            .as_i64()
            .unwrap();
        let iid = p.layer(KEY, 0).unwrap()["entityInstances"][0]["iid"]
            .as_str()
            .unwrap()
            .to_owned();
        let field = p
            .add_field_definition(Some(uid), "Path", "Point", true)
            .unwrap();
        p.set_field_value(KEY, Some(&iid), field, json!([{"cx":4,"cy":2}]))
            .unwrap();
        app.erase_entities_at_input(&ctx, Rect::EVERYTHING, [72., 40.], true);
        let shapes = draw(&app, &ctx, 0.);
        assert_eq!(shapes.len(), 2);
        let circle = shapes
            .iter()
            .find_map(|s| {
                if let egui::Shape::Circle(c) = s {
                    Some(c)
                } else {
                    None
                }
            })
            .unwrap();
        assert_eq!(circle.center, egui::pos2(272., 240.));
        assert_eq!(
            app.project.as_ref().unwrap().layer(KEY, 0).unwrap()["entityInstances"][0]["fieldInstances"]
                [0]["__value"],
            json!([])
        );
    }
    #[test]
    fn entity_erase_uses_snapshot_after_removal() {
        let (mut app, ctx) = fixture();
        app.erase_entity([24., 24.]);
        assert!(
            app.project.as_ref().unwrap().layer(KEY, 0).unwrap()["entityInstances"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        assert_eq!(draw(&app, &ctx, 0.).len(), 1);
    }
    #[test]
    fn hidden_invalid_tags_bleep_with_source_extra_scale_speed_and_inspector_suppression() {
        let (mut app, ctx) = fixture();
        let p = app.project.as_mut().unwrap();
        p.json["defs"]["layers"][0]["requiredTags"] = json!(["Required"]);
        p.layer_mut(KEY, 0).unwrap()["visible"] = json!(false);
        app.panel = Some(Panel::Entity);
        app.entity_index = Some(0);
        assert!(draw(&app, &ctx, 0.).is_empty());
        app.panel = None;
        let shapes = draw(&app, &ctx, 0.7);
        assert_eq!(shapes.len(), 1);
        let egui::Shape::Rect(rect) = &shapes[0] else {
            panic!()
        };
        assert!((rect.rect.width() - 67.6).abs() < 0.001);
        assert_eq!(rect.stroke.color, Color32::RED);
        assert!(
            !draw(&app, &ctx, 1.2).is_empty(),
            "speed .4 lasts longer than ordinary bleep"
        );
        app.project.as_mut().unwrap().json["defs"]["entities"][0]["tags"] = json!(["Required"]);
        assert!(draw(&app, &ctx, 1.4).is_empty());
    }
    #[test]
    fn shared_level_error_changes_from_tag_to_field_and_clears_after_repair() {
        let (mut app, ctx) = fixture();
        let project = app.project.as_mut().unwrap();
        let layer = project.layer(KEY, 0).unwrap();
        let layer_uid = layer["layerDefUid"].as_i64().unwrap();
        let entity = &layer["entityInstances"][0];
        let iid = entity["iid"].as_str().unwrap().to_owned();
        let def = entity["defUid"].as_i64().unwrap();
        let name = entity["__identifier"].as_str().unwrap().to_owned();
        let field = project
            .add_field_definition(Some(def), "RequiredPoint", "Point", false)
            .unwrap();
        project
            .update_field_definition(field, &json!({"canBeNull":false}))
            .unwrap();
        project
            .update_layer_properties(layer_uid, &json!({"requiredTags":["Required"]}))
            .unwrap();
        let check = |project: &Project, time| {
            let mut issue = None;
            let _ = ctx.run(
                egui::RawInput {
                    time: Some(time),
                    ..Default::default()
                },
                |ctx| {
                    issue = level_entity_error(ctx, project, KEY);
                },
            );
            issue
        };
        assert_eq!(check(project, 0.), Some(format!("Incorrect tag: {name}")));
        project
            .update_entity_definition(def, &json!({"tags":["Required"]}))
            .unwrap();
        assert_eq!(
            check(project, 1.),
            Some(format!("Invalid field value: {name}"))
        );
        project
            .set_field_value(KEY, Some(&iid), field, json!({"cx":4,"cy":2}))
            .unwrap();
        assert_eq!(check(project, 2.), None);
    }

    #[test]
    fn error_scan_uses_last_error_layer_first_entity_even_when_offscreen() {
        let (mut app, ctx) = fixture();
        let p = app.project.as_mut().unwrap();
        let uid = p.layer(KEY, 0).unwrap()["entityInstances"][0]["defUid"]
            .as_i64()
            .unwrap();
        let field = p
            .add_field_definition(Some(uid), "RequiredPoint", "Point", false)
            .unwrap();
        p.update_field_definition(field, &json!({"canBeNull":false}))
            .unwrap();
        let layer = p.add_layer("Back", "Entities", 16).unwrap();
        let index = p.level(KEY).unwrap()["layerInstances"]
            .as_array()
            .unwrap()
            .iter()
            .position(|li| li["layerDefUid"] == layer)
            .unwrap();
        p.resize_level(KEY, 4096, 4096, 0, 0).unwrap();
        assert!(p.add_entity(KEY, index, uid, 1000, 1000).unwrap());
        assert!(p.add_entity(KEY, index, uid, 2000, 2000).unwrap());
        p.reorder_layer(layer, 1).unwrap();
        let state_id = error_scan_id(p, KEY).unwrap();
        let mut scan = ErrorScan {
            running: true,
            ..Default::default()
        };
        // Force a yield after the first layer; its error must not be queued while
        // a later layer can still replace it, regardless of the machine's speed.
        assert!(!advance_error_scan(
            p,
            KEY,
            &mut scan,
            std::time::Duration::ZERO
        ));
        assert_eq!(scan.layer, 1);
        assert_eq!(
            scan.candidate.as_ref().unwrap().layer_uid,
            p.layer(KEY, 0).unwrap()["layerDefUid"]
        );
        assert!(ctx.data(|d| d.get_temp::<Queue>(id())).is_none());
        ctx.data_mut(|d| d.insert_temp(state_id, scan));
        // The remaining entity is enough to complete the scan in this frame.
        draw(&app, &ctx, 0.);
        assert!(ctx.data(|d| { !d.get_temp::<ErrorScan>(state_id).unwrap().running }));
        let queue = ctx
            .data(|d| d.get_temp::<Queue>(id()))
            .expect("offscreen error still queues feedback");
        assert_eq!(queue.items.len(), 1);
        assert_eq!(queue.items[0].center, [1000., 992.]);
    }

    #[test]
    fn zero_budget_yields_between_entities_and_finishes_after_the_last() {
        let (mut app, _) = fixture();
        let p = app.project.as_mut().unwrap();
        let uid = p.layer(KEY, 0).unwrap()["entityInstances"][0]["defUid"]
            .as_i64()
            .unwrap();
        p.add_entity(KEY, 0, uid, 48, 48).unwrap();
        p.add_entity(KEY, 0, uid, 64, 64).unwrap();
        let empty = p.add_layer("Empty", "IntGrid", 16).unwrap();
        p.reorder_layer(empty, 1).unwrap();
        let mut scan = ErrorScan::default();
        for expected in 1..3 {
            assert!(!advance_error_scan(
                p,
                KEY,
                &mut scan,
                std::time::Duration::ZERO
            ));
            assert_eq!(scan.entity, expected);
        }
        assert!(advance_error_scan(
            p,
            KEY,
            &mut scan,
            std::time::Duration::ZERO
        ));
        assert_eq!(scan.layer, 2);
        assert!(scan.candidate.is_none());
    }
}
