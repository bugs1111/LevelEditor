use super::*;
use serde_json::json;

pub(super) struct FieldPick {
    key: LevelKey,
    owner: Option<String>,
    uid: i64,
    kind: String,
    index: Option<usize>,
    array: bool,
    panel: Option<Panel>,
    layer: usize,
}

impl FieldPick {
    fn owner<'a>(&self, project: &'a Project) -> Option<&'a Value> {
        if let Some(iid) = &self.owner {
            project.layer(self.key, self.layer)?["entityInstances"]
                .as_array()?
                .iter()
                .find(|e| e["iid"] == *iid)
        } else {
            project.level(self.key)
        }
    }
    fn definition<'a>(&self, project: &'a Project) -> Option<&'a Value> {
        project.json["defs"]["entities"]
            .as_array()
            .into_iter()
            .flatten()
            .flat_map(|d| d["fieldDefs"].as_array().into_iter().flatten())
            .chain(
                project.json["defs"]["levelFields"]
                    .as_array()
                    .into_iter()
                    .flatten(),
            )
            .find(|d| d["uid"] == self.uid)
    }
    fn accepts(&self, project: &Project, key: LevelKey, target: &Value) -> bool {
        let Some(definition) = self.definition(project) else {
            return false;
        };
        let Some(owner) = self.owner(project) else {
            return false;
        };
        if self.owner.as_deref() == target["iid"].as_str()
            || definition["allowOutOfLevelRef"] == false && key != self.key
        {
            return false;
        }
        let existing = owner["fieldInstances"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|f| f["defUid"] == self.uid);
        if existing.is_some_and(|field| {
            let values = if self.array {
                field["__value"]
                    .as_array()
                    .map(Vec::as_slice)
                    .unwrap_or_default()
            } else {
                std::slice::from_ref(&field["__value"])
            };
            values.iter().any(|v| v["entityIid"] == target["iid"])
        }) {
            return false;
        }
        match definition["allowedRefs"].as_str().unwrap_or("Any") {
            "OnlySame" => owner["defUid"] == target["defUid"],
            "OnlySpecificEntity" => target["defUid"] == definition["allowedRefsEntityUid"],
            "OnlyTags" => project.json["defs"]["entities"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|d| d["uid"] == target["defUid"])
                .is_some_and(|d| {
                    definition["allowedRefTags"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .any(|tag| d["tags"].as_array().is_some_and(|tags| tags.contains(tag)))
                }),
            _ => true,
        }
    }
}

impl EditorApp {
    fn reference_pick_levels(&self, pick: &FieldPick) -> Vec<(LevelKey, [f32; 4])> {
        let Some(project) = &self.project else {
            return Vec::new();
        };
        if self.world {
            if pick
                .definition(project)
                .is_some_and(|d| d["allowOutOfLevelRef"] == false)
            {
                return Vec::new();
            }
            canvas::placements(project, self.selected.unwrap_or(pick.key))
                .into_iter()
                .filter(|(key, _)| {
                    project
                        .level(*key)
                        .is_some_and(|l| l["worldDepth"].as_i64().unwrap_or(0) == self.world_depth)
                })
                .collect()
        } else {
            vec![(
                self.selected.unwrap_or(pick.key),
                [
                    f32::NEG_INFINITY,
                    f32::NEG_INFINITY,
                    f32::INFINITY,
                    f32::INFINITY,
                ],
            )]
        }
    }

    pub(super) fn draw_field_pick_targets(&self, ui: &egui::Ui, rect: Rect) {
        let Some(pick) = self.field_pick.as_ref().filter(|p| p.kind == "EntityRef") else {
            return;
        };
        let Some(project) = &self.project else { return };
        let painter = ui.painter().with_clip_rect(rect);
        let alpha = ui.input(|i| (i.time as f32 * 4.2).cos().abs());
        let mut count = 0;
        for (key, bounds) in self.reference_pick_levels(pick) {
            let Some(layers) = project
                .level(key)
                .and_then(|l| l["layerInstances"].as_array())
            else {
                continue;
            };
            for (index, layer) in layers.iter().enumerate() {
                if !self.layer_visible(key, index) || layer["__type"] != "Entities" {
                    continue;
                }
                for entity in layer["entityInstances"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter(|e| pick.accepts(project, key, e))
                {
                    count += 1;
                    let width = number(entity, "width", 16.);
                    let height = number(entity, "height", 16.);
                    let local = [
                        entity["px"][0].as_f64().unwrap_or(0.) as f32
                            + (0.5 - entity["__pivot"][0].as_f64().unwrap_or(0.) as f32) * width,
                        entity["px"][1].as_f64().unwrap_or(0.) as f32
                            + (0.5 - entity["__pivot"][1].as_f64().unwrap_or(0.) as f32) * height,
                    ];
                    let (position, scale) = if self.world {
                        (
                            [
                                bounds[0] + number(layer, "__pxTotalOffsetX", 0.) + local[0],
                                bounds[1] + number(layer, "__pxTotalOffsetY", 0.) + local[1],
                            ],
                            1.,
                        )
                    } else {
                        let a =
                            canvas::layer_to_world(project, key, index, &self.viewport, [0., 0.]);
                        let b =
                            canvas::layer_to_world(project, key, index, &self.viewport, [1., 0.]);
                        (
                            canvas::layer_to_world(project, key, index, &self.viewport, local),
                            (b[0] - a[0]).abs(),
                        )
                    };
                    let center = self
                        .viewport
                        .to_screen(position, rect.min.into(), rect.size().into())
                        .into();
                    for (padding, opacity) in [(8., 1.), (12., 0.33)] {
                        painter.circle_stroke(
                            center,
                            (width.max(height) * 0.5 + padding) * scale * self.viewport.zoom,
                            Stroke::new(
                                self.viewport.zoom,
                                Color32::from_rgba_unmultiplied(
                                    255,
                                    204,
                                    0,
                                    (255. * alpha * opacity) as u8,
                                ),
                            ),
                        );
                    }
                }
            }
        }
        let same_level_only = pick
            .definition(project)
            .is_some_and(|d| d["allowOutOfLevelRef"] == false);
        let message = if same_level_only && (self.world || self.selected != Some(pick.key)) {
            "You can only pick a reference in the same level for this value."
        } else if count == 0 {
            "No valid target in this level."
        } else {
            "Pick a highlighted entity · Right click / Escape: return to source"
        };
        painter.text(
            rect.center_top() + egui::vec2(0., 18.),
            Align2::CENTER_TOP,
            message,
            FontId::proportional(14.),
            if count == 0 {
                Color32::LIGHT_RED
            } else {
                theme::TEXT
            },
        );
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(33));
    }
    pub(super) fn picking_entity_ref(&self) -> bool {
        self.field_pick
            .as_ref()
            .is_some_and(|p| p.kind == "EntityRef")
    }
    pub(super) fn field_pick_controls(
        &mut self,
        ui: &mut egui::Ui,
        key: LevelKey,
        owner: Option<&str>,
        field: &Value,
        definition: &Value,
    ) -> Option<Rect> {
        let kind = text(field, "__type", "");
        let inner = kind
            .strip_prefix("Array<")
            .and_then(|s| s.strip_suffix('>'))
            .unwrap_or(kind);
        if !matches!(inner, "Point" | "EntityRef") {
            return None;
        }
        let array = kind.starts_with("Array<");
        let mut pick_rect = None;
        let mut requested = None;
        let mut follow = None;
        let mut edit = None;
        let values = if array {
            field["__value"]
                .as_array()
                .map(Vec::as_slice)
                .unwrap_or_default()
        } else {
            std::slice::from_ref(&field["__value"])
        };
        let path = array
            && inner == "Point"
            && matches!(
                text(definition, "editorDisplayMode", ""),
                "Points" | "PointPath" | "PointStar"
            );
        let sort_id = ui.id().with("reference-array-sort");
        if path && !values.is_empty() {
            let summary = values
                .iter()
                .map(|value| format!("<{},{}>", value["cx"], value["cy"]))
                .collect::<Vec<_>>()
                .join(", ");
            ui.add(egui::Label::new(summary).wrap());
        } else {
            for (index, value) in values.iter().enumerate() {
                ui.push_id(index, |ui| {
                    let row = ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 2.;
                        if array && inner != "Point" {
                            ui.dnd_drag_source(
                                ui.id().with("sort-reference"),
                                (sort_id, index),
                                |ui| {
                                    ui.add_sized(
                                        [12., 24.],
                                        egui::Label::new("≡").sense(Sense::drag()),
                                    );
                                },
                            );
                        }
                        let width = (ui.available_width()
                            - if inner == "EntityRef" && !value.is_null() {
                                44.
                            } else {
                                22.
                            })
                        .max(24.);
                        if inner == "EntityRef" {
                            let iid = value["entityIid"].as_str();
                            let label = iid
                                .and_then(|iid| {
                                    self.project.as_ref().and_then(|project| {
                                        project.level_keys().into_iter().find_map(|key| {
                                            project.level(key)?["layerInstances"]
                                                .as_array()?
                                                .iter()
                                                .flat_map(|layer| {
                                                    layer["entityInstances"]
                                                        .as_array()
                                                        .into_iter()
                                                        .flatten()
                                                })
                                                .find(|entity| entity["iid"] == iid)
                                                .map(|entity| {
                                                    text(entity, "__identifier", "Entity")
                                                        .to_owned()
                                                })
                                        })
                                    })
                                })
                                .unwrap_or_else(|| "Missing reference".into());
                            let response =
                                ui.add_sized([width, 28.], egui::Button::new(label).truncate());
                            if let Some(iid) = iid {
                                self.hover_entity_reference(&response, iid);
                                if response.clicked() {
                                    follow = Some(iid.to_owned());
                                }
                            } else {
                                if response.clicked() {
                                    requested = Some(if array { Some(index) } else { None });
                                }
                            }
                            let response = ui
                                .add_sized(
                                    [20., 24.],
                                    egui::Button::image(
                                        egui::Image::new(egui::include_image!(
                                            "../../assets/icons/edit.svg"
                                        ))
                                        .fit_to_exact_size(egui::vec2(16., 16.)),
                                    )
                                    .frame(false),
                                )
                                .on_hover_text("Pick from canvas");
                            pick_rect = Some(response.rect);
                            if response.clicked() {
                                requested = Some(if array { Some(index) } else { None });
                            }
                        } else {
                            let label = if value.is_null() {
                                "Pick a point".into()
                            } else {
                                format!("<{},{}>", value["cx"], value["cy"])
                            };
                            let response = ui
                                .add_sized([width, 28.], egui::Button::new(label))
                                .on_hover_text("Pick from canvas");
                            pick_rect = Some(response.rect);
                            if response.clicked() {
                                requested = Some(if array { Some(index) } else { None });
                            }
                        }
                        if (array || definition["canBeNull"] == true && !value.is_null())
                            && ui
                                .small_button("×")
                                .on_hover_text(if array {
                                    "Remove array value"
                                } else {
                                    "Clear field value"
                                })
                                .clicked()
                        {
                            edit = Some(if array {
                                ui_fields::FieldEdit::Array(ldtk_core::FieldArrayEdit::Remove {
                                    index,
                                })
                            } else {
                                ui_fields::FieldEdit::Value(Value::Null)
                            });
                        }
                    });
                    if let Some(payload) = row.response.dnd_release_payload::<(egui::Id, usize)>()
                        && payload.0 == sort_id
                        && payload.1 != index
                    {
                        edit = Some(ui_fields::FieldEdit::Array(
                            ldtk_core::FieldArrayEdit::Move {
                                from: payload.1,
                                to: index,
                            },
                        ));
                    }
                });
            }
        }
        if array
            && (values.len() as u64) < definition["arrayMaxLength"].as_u64().unwrap_or(u64::MAX)
        {
            let response = ui
                .add_sized([ui.available_width(), 22.], egui::Button::new("+"))
                .on_hover_text("Add from canvas");
            pick_rect = Some(response.rect);
            if response.clicked() {
                requested = Some(None);
            }
        }
        if let Some(value) = edit {
            let uid = definition["uid"].as_i64().unwrap_or(-1);
            if owner.is_some() {
                self.edit_level("Field changed", key, |project| {
                    value.apply(project, key, owner, uid)
                });
            } else if self
                .edit("Field changed", |project| {
                    value.apply(project, key, owner, uid)
                })
                .is_some()
            {
                self.history.clear_level_timelines();
            }
        }
        if let Some(iid) = follow {
            self.follow_entity(&iid);
        }
        if let Some(index) = requested {
            self.finish_gesture();
            self.finish_stroke();
            self.field_pick = Some(FieldPick {
                key,
                owner: owner.map(str::to_owned),
                uid: definition["uid"].as_i64().unwrap_or(-1),
                kind: inner.into(),
                index,
                array,
                panel: self.panel,
                layer: self.layer,
            });
            self.panel = None;
            self.stamp = None;
            self.status =
                "Click the canvas to pick a field value · Right click / Escape: finish".into();
        }
        pick_rect
    }

    pub(super) fn cancel_field_pick(&mut self) {
        if let Some(pick) = self.field_pick.take() {
            if pick.kind == "EntityRef" {
                if let Some(iid) = &pick.owner {
                    self.follow_entity(iid);
                } else {
                    self.navigate_level(pick.key, true);
                    self.layer = pick.layer;
                }
            }
            self.panel = pick.panel;
        }
    }

    pub(super) fn field_pick_input(
        &mut self,
        ctx: &egui::Context,
        rect: Rect,
        response: &egui::Response,
    ) -> bool {
        let Some(pick) = &self.field_pick else {
            return false;
        };
        ctx.set_cursor_icon(egui::CursorIcon::Crosshair);
        if ctx.input(|i| i.key_pressed(Key::Escape) || i.pointer.secondary_clicked()) {
            self.cancel_field_pick();
            return true;
        }
        if !response.hovered() || !ctx.input(|i| i.pointer.primary_clicked()) {
            return true;
        }
        let Some(pointer) = response.interact_pointer_pos() else {
            return true;
        };
        let Some(project) = &self.project else {
            return true;
        };
        let position = self
            .viewport
            .to_world(pointer.into(), rect.min.into(), rect.size().into());
        let value = if pick.kind == "Point" {
            let grid = project
                .layer(pick.key, pick.layer)
                .map(|l| number(l, "__gridSize", 16.0))
                .unwrap_or_else(|| number(&project.json, "defaultGridSize", 16.0))
                .max(1.0);
            let origin = if self.world {
                canvas::placements(project, pick.key)
                    .into_iter()
                    .find(|(k, _)| *k == pick.key)
                    .map(|(_, b)| [b[0], b[1]])
                    .unwrap_or([0.0, 0.0])
            } else {
                [0.0, 0.0]
            };
            let layer = project.layer(pick.key, pick.layer);
            let local = if self.world {
                [
                    position[0]
                        - origin[0]
                        - layer
                            .map(|l| number(l, "__pxTotalOffsetX", 0.0))
                            .unwrap_or(0.0),
                    position[1]
                        - origin[1]
                        - layer
                            .map(|l| number(l, "__pxTotalOffsetY", 0.0))
                            .unwrap_or(0.0),
                ]
            } else {
                canvas::world_to_layer(project, pick.key, pick.layer, &self.viewport, position)
            };
            let cell = local.map(|v| (v / grid).floor() as i32);
            let width = layer
                .map(|l| number(l, "__cWid", 0.0))
                .unwrap_or_else(|| number(project.level(pick.key).unwrap(), "pxWid", 0.0) / grid)
                as i32;
            let height = layer
                .map(|l| number(l, "__cHei", 0.0))
                .unwrap_or_else(|| number(project.level(pick.key).unwrap(), "pxHei", 0.0) / grid)
                as i32;
            if cell[0] < 0 || cell[1] < 0 || cell[0] >= width || cell[1] >= height {
                return true;
            }
            json!({"cx":cell[0],"cy":cell[1]})
        } else {
            let levels = self.reference_pick_levels(pick);
            let mut target = None;
            for (key, bounds) in levels.into_iter().rev() {
                if position[0] < bounds[0]
                    || position[1] < bounds[1]
                    || position[0] >= bounds[2]
                    || position[1] >= bounds[3]
                {
                    continue;
                }
                if let Some(level) = project.level(key)
                    && let Some(layers) = level["layerInstances"].as_array()
                {
                    for (index, layer) in layers.iter().enumerate() {
                        if !self.layer_visible(key, index) || layer["__type"] != "Entities" {
                            continue;
                        }
                        let local = if !self.world {
                            canvas::world_to_layer(project, key, index, &self.viewport, position)
                        } else {
                            [
                                position[0] - bounds[0] - number(layer, "__pxTotalOffsetX", 0.0),
                                position[1] - bounds[1] - number(layer, "__pxTotalOffsetY", 0.0),
                            ]
                        };
                        if let Some(entity) = layer["entityInstances"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .find(|entity| {
                                pick.accepts(project, key, entity)
                                    && selection_click::entity_is_over(project, entity, local)
                            })
                        {
                            target = Some(
                                json!({"entityIid":entity["iid"],"layerIid":layer["iid"],"levelIid":level["iid"],"worldIid":key.world.and_then(|w|project.json["worlds"][w]["iid"].as_str()).or_else(||project.json["dummyWorldIid"].as_str())}),
                            );
                            break;
                        }
                    }
                }
                if target.is_some() {
                    break;
                }
            }
            let Some(value) = target else {
                self.status = "Pick an entity, or press Escape to finish".into();
                return true;
            };
            value
        };
        let point_cell = (pick.kind == "Point").then(|| {
            [
                value["cx"].as_i64().unwrap_or(0) as i32,
                value["cy"].as_i64().unwrap_or(0) as i32,
            ]
        });
        let key = pick.key;
        let layer = pick.layer;
        let owner = pick.owner.clone();
        let uid = pick.uid;
        let close = pick.kind == "EntityRef" || !pick.array || pick.index.is_some();
        let next = if pick.array {
            let field_owner = if let Some(iid) = &owner {
                project
                    .layer(key, layer)
                    .and_then(|l| l["entityInstances"].as_array())
                    .and_then(|a| a.iter().find(|e| e["iid"] == iid.as_str()))
            } else {
                project.level(key)
            };
            let mut array = field_owner
                .and_then(|o| o["fieldInstances"].as_array())
                .and_then(|a| a.iter().find(|f| f["defUid"] == uid))
                .and_then(|f| f["__value"].as_array())
                .cloned()
                .unwrap_or_default();
            if let Some(index) = pick.index {
                if let Some(item) = array.get_mut(index) {
                    *item = value;
                }
            } else {
                array.push(value);
            }
            json!(array)
        } else {
            value
        };
        let result = if owner.is_some() {
            self.edit_level_marked("Field picked", key, |p, history| {
                if let Some(entity) = p
                    .layer(key, layer)
                    .and_then(|l| l["entityInstances"].as_array())
                    .and_then(|es| es.iter().find(|e| e["iid"].as_str() == owner.as_deref()))
                {
                    history.mark_entity_change(p, key, layer, entity);
                }
                p.set_field_editor_value(key, owner.as_deref(), uid, next)
            })
        } else {
            let result = self.edit("Field picked", |p| {
                p.set_field_editor_value(key, None, uid, next)
            });
            if result.is_some() {
                self.history.clear_level_timelines();
            }
            result
        };
        if result.is_some()
            && let Some(cell) = point_cell
        {
            self.bleep_layer_cells(key, layer, [cell[0], cell[1], 1, 1], theme::ACCENT);
        }
        if result.is_some() && close {
            self.cancel_field_pick();
        }

        true
    }
}

#[cfg(test)]
#[path = "field_tools_tests.rs"]
mod tests;
