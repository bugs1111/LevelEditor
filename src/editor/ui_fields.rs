use super::*;
use serde_json::json;

pub(super) enum Widget {
    Text,
    Multiline,
    Int(i64, i64),
    Float(f64, f64),
    Bool,
    Color,
    NullableColor,
    Choice(&'static [&'static str]),
    Path,
}
pub(super) struct Property(pub &'static str, pub &'static str, pub Widget);

#[derive(Clone)]
struct DialogContext {
    project: Option<PathBuf>,
    directories: Value,
}
#[derive(Clone)]
struct DialogChoice {
    project: Option<PathBuf>,
    ui_id: String,
    path: PathBuf,
}
fn dialog_context_id() -> egui::Id {
    egui::Id::new("source-form-dialog-context")
}
pub(super) fn prepare_dialog_context(app: &mut EditorApp, ctx: &egui::Context) {
    if let Some(choice) = ctx.data_mut(|d| {
        let id = dialog_context_id().with("choice");
        let value = d.get_temp::<DialogChoice>(id);
        d.remove::<DialogChoice>(id);
        value
    }) {
        app.settings
            .store_ui_dir(choice.project.as_deref(), &choice.ui_id, &choice.path);
    }
    ctx.data_mut(|d| {
        d.insert_temp(
            dialog_context_id(),
            DialogContext {
                project: app.project.as_ref().and_then(|p| p.path.clone()),
                directories: app.settings.value["lastUiDirs"].clone(),
            },
        )
    });
}
pub(super) fn pick_file(ui: &egui::Ui, ui_id: &str, extensions: &[&str]) -> Option<PathBuf> {
    let context = ui
        .ctx()
        .data_mut(|d| d.get_temp::<DialogContext>(dialog_context_id()));
    let mut dialog = rfd::FileDialog::new();
    if !extensions.is_empty() {
        dialog = dialog.add_filter("File", extensions);
    }
    if let Some(context) = &context {
        let settings = super::settings::Settings {
            value: json!({"lastUiDirs":context.directories}),
            path: None,
        };
        if let Some(dir) = settings.ui_dir(context.project.as_deref(), ui_id, None) {
            dialog = dialog.set_directory(dir);
        }
    }
    let path = dialog.pick_file()?;
    if let Some(parent) = path.parent() {
        ui.ctx().data_mut(|d| {
            d.insert_temp(
                dialog_context_id().with("choice"),
                DialogChoice {
                    project: context.and_then(|c| c.project),
                    ui_id: ui_id.into(),
                    path: parent.to_owned(),
                },
            )
        });
    }
    Some(path)
}
fn numeric_optional(key: &str) -> bool {
    matches!(
        key,
        "min"
            | "max"
            | "minWidth"
            | "maxWidth"
            | "minHeight"
            | "maxHeight"
            | "arrayMinLength"
            | "arrayMaxLength"
            | "outOfBoundsValue"
    )
}

pub(super) fn buffered_text(
    ui: &mut egui::Ui,
    key: impl std::hash::Hash,
    current: &str,
) -> Option<String> {
    buffered_text_kind(ui, key, current, false)
}

fn buffered_text_kind(
    ui: &mut egui::Ui,
    key: impl std::hash::Hash,
    current: &str,
    multiline: bool,
) -> Option<String> {
    let id = ui.make_persistent_id(key);
    let (mut original, mut draft) = ui
        .data_mut(|d| d.get_temp::<(String, String)>(id))
        .unwrap_or_else(|| (current.into(), current.into()));
    if original != current {
        original = current.into();
        draft = current.into();
    }
    let edit = if multiline {
        egui::TextEdit::multiline(&mut draft)
    } else {
        egui::TextEdit::singleline(&mut draft)
    };
    ui_commit::prepare_editor(ui.ctx(), id);
    let response = ui.add(edit.id(id).desired_width(240.0));
    ui_commit::track_editor(ui.ctx(), &response);
    track_default_editor(ui, &response);
    let commit = (response.lost_focus()
        || !multiline && response.has_focus() && ui.input(|i| i.key_pressed(Key::Enter)))
        && draft != current;
    ui.data_mut(|d| {
        if commit {
            d.remove::<(String, String)>(id);
        } else {
            d.insert_temp(id, (original, draft.clone()));
        }
    });
    commit.then_some(draft)
}

pub(super) fn form(ui: &mut egui::Ui, value: &Value, properties: &[Property]) -> Value {
    let mut patch = serde_json::Map::new();
    egui::Grid::new(
        ui.id()
            .with(("property-form", properties.first().map(|p| p.0))),
    )
    .num_columns(2)
    .spacing([18.0, 8.0])
    .show(ui, |ui| {
        for Property(key, label, widget) in properties {
            super::panels::source_controls::property_label(ui, label, key);
            let mut current = value[*key].clone();
            let changed = match widget {
                Widget::Text => {
                    if let Some(v) = buffered_text(ui, *key, current.as_str().unwrap_or("")) {
                        current = json!(v);
                        true
                    } else {
                        false
                    }
                }
                Widget::Multiline => {
                    if let Some(v) =
                        buffered_text_kind(ui, *key, current.as_str().unwrap_or(""), true)
                    {
                        current = json!(v);
                        true
                    } else {
                        false
                    }
                }
                Widget::Int(min, max) => {
                    let mut v = current.as_i64().unwrap_or(0).clamp(*min, *max);
                    let mut c = false;
                    ui.horizontal(|ui| {
                        let optional = numeric_optional(key);
                        let mut enabled = !current.is_null();
                        if optional && ui.checkbox(&mut enabled, "").changed() {
                            current = if enabled { json!(v) } else { Value::Null };
                            c = true;
                        }
                        ui_commit::prepare_editor(ui.ctx(), ui.next_auto_id());
                        let response = ui.add_enabled(
                            !optional || enabled,
                            egui::DragValue::new(&mut v)
                                .range(*min..=*max)
                                .update_while_editing(false),
                        );
                        ui_commit::track_editor(ui.ctx(), &response);
                        if response.changed() {
                            current = json!(v);
                            c = true;
                        }
                    });
                    c
                }
                Widget::Float(min, max) => {
                    let mut v = current.as_f64().unwrap_or(0.0).clamp(*min, *max);
                    let mut c = false;
                    ui.horizontal(|ui| {
                        let optional = numeric_optional(key);
                        let mut enabled = !current.is_null();
                        if optional && ui.checkbox(&mut enabled, "").changed() {
                            current = if enabled { json!(v) } else { Value::Null };
                            c = true;
                        }
                        ui_commit::prepare_editor(ui.ctx(), ui.next_auto_id());
                        let response = ui.add_enabled(
                            !optional || enabled,
                            egui::DragValue::new(&mut v)
                                .speed(0.01)
                                .range(*min..=*max)
                                .update_while_editing(false),
                        );
                        ui_commit::track_editor(ui.ctx(), &response);
                        if response.changed() {
                            current = json!(v);
                            c = true;
                        }
                    });
                    c
                }
                Widget::Bool => {
                    let mut v = current.as_bool().unwrap_or(false);
                    let c = ui.checkbox(&mut v, "").changed();
                    if c {
                        current = json!(v);
                    }
                    c
                }
                Widget::Color => {
                    let col = theme::color(current.as_str().unwrap_or("#FFFFFF"));
                    let mut rgb = [col.r(), col.g(), col.b()];
                    let c = super::panels::source_colors::color_editor_tag(
                        ui,
                        &mut rgb,
                        if *key == "bgColor" { Some("bg") } else { None },
                    )
                    .changed();
                    if c {
                        current = json!(format!("#{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2]));
                    }
                    c
                }
                Widget::NullableColor => {
                    let mut rgb = current
                        .as_str()
                        .map(theme::color)
                        .map(|c| [c.r(), c.g(), c.b()]);
                    if super::panels::source_colors::nullable_color_editor(ui, &mut rgb).changed() {
                        current = rgb
                            .map(|rgb| {
                                json!(format!("#{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2]))
                            })
                            .unwrap_or(Value::Null);
                        true
                    } else {
                        false
                    }
                }
                Widget::Choice(choices) => {
                    let mut v = current.as_str().unwrap_or(choices[0]).to_owned();
                    let old = v.clone();
                    egui::ComboBox::from_id_salt(ui.id().with(*key))
                        .selected_text(&v)
                        .show_ui(ui, |ui| {
                            for option in *choices {
                                ui.selectable_value(&mut v, (*option).into(), *option);
                            }
                        });
                    if v != old {
                        current = json!(v);
                        true
                    } else {
                        false
                    }
                }
                Widget::Path => {
                    let mut v = current.as_str().unwrap_or("").to_owned();
                    let mut c = false;
                    ui.horizontal(|ui| {
                        if let Some(next) = buffered_text(ui, *key, &v) {
                            v = next;
                            c = true;
                        }
                        if super::panels::source_controls::small_button(ui, "…").clicked()
                            && let Some(p) = pick_file(
                                ui,
                                if *key == "bgRelPath" || *key == "relPath" {
                                    "PickImage"
                                } else {
                                    key
                                },
                                &[],
                            )
                        {
                            v = p.to_string_lossy().into_owned();
                            c = true;
                        }
                        if super::panels::source_controls::small_button(ui, "×").clicked() {
                            v.clear();
                            c = true;
                        }
                    });
                    if c {
                        current = if v.is_empty() { Value::Null } else { json!(v) };
                    }
                    c
                }
            };
            if changed {
                patch.insert((*key).into(), current);
            }
            ui.end_row();
        }
    });
    Value::Object(patch)
}

pub(super) fn apply_patch(target: &mut Value, patch: &Value) {
    if let Some(patch) = patch.as_object() {
        for (k, v) in patch {
            target[k] = v.clone();
        }
    }
}
pub(super) fn changed(patch: &Value) -> bool {
    patch.as_object().is_some_and(|p| !p.is_empty())
}

pub(super) fn pivot_editor(
    ui: &mut egui::Ui,
    key: impl std::hash::Hash,
    pivot: [f32; 2],
    size: [f32; 2],
) -> Option<[f32; 2]> {
    let mut result = None;
    ui.push_id(key,|ui| {
        let (rect,response)=ui.allocate_exact_size(Vec2::splat(64.),Sense::click());
        ui.painter().rect_filled(rect,0.,theme::DARK);
        ui.painter().rect_stroke(rect,0.,Stroke::new(1_f32,theme::LIGHT),StrokeKind::Inside);
        for y in 0..3 { for x in 0..3 {
            let point=rect.min+egui::vec2(8.+24.*x as f32,8.+24.*y as f32);
            let selected=pivot==[x as f32*0.5,y as f32*0.5];
            ui.painter().circle_filled(point,if selected{5.}else{3.},if selected{theme::ACCENT}else{theme::LIGHT});
        }}
        if response.clicked() && let Some(pos)=response.interact_pointer_pos() {
            result=Some([((pos.x-rect.left()-8.)/24.).round().clamp(0.,2.)*0.5,((pos.y-rect.top()-8.)/24.).round().clamp(0.,2.)*0.5]);
        }
        let id=ui.id().with("advanced-pivot");
        let standard=|v:f32|[0.,0.5,1.].contains(&v);
        let mut advanced=ui.ctx().data_mut(|d|d.get_temp::<bool>(id)).unwrap_or(!standard(pivot[0])||!standard(pivot[1]));
        if !advanced && panels::source_controls::small_button(ui,"Advanced…").clicked() { advanced=true; }
        ui.ctx().data_mut(|d|d.insert_temp(id,advanced));
        if advanced {
            let value=json!({"x":pivot[0],"y":pivot[1],"px":(pivot[0]*size[0]).floor(),"py":(pivot[1]*size[1]).floor()});
            let patch=form(ui,&value,&[Property("x","Pivot X",Widget::Float(-50.,50.)),Property("y","Pivot Y",Widget::Float(-50.,50.)),Property("px","X pixels",Widget::Int(-2048,2048)),Property("py","Y pixels",Widget::Int(-2048,2048))]);
            if changed(&patch) {
                let mut p=pivot;
                for (axis,(ratio,pixels)) in [("x","px"),("y","py")].into_iter().enumerate() {
                    if let Some(v)=patch[ratio].as_f64() {p[axis]=v as f32;}
                    if let Some(v)=patch[pixels].as_f64() {p[axis]=v as f32/size[axis].max(1.);}
                }
                result=Some(p);
            }
        }
    });
    result
}

pub(super) fn tags(ui: &mut egui::Ui, label: &str, value: &Value) -> Option<Value> {
    let mut text = value
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(", ")
        })
        .unwrap_or_default();
    let mut result = None;
    ui.horizontal(|ui| {
        ui.label(label);
        if ui.text_edit_singleline(&mut text).changed() {
            result = Some(json!(
                text.split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>()
            ));
        }
    });
    result
}

pub(super) fn scalar_editor(
    ui: &mut egui::Ui,
    project: &Project,
    kind: &str,
    value: &Value,
    definition: &Value,
) -> Option<Value> {
    scalar_editor_inner(ui, project, kind, value, definition, true, false)
}

fn scalar_editor_inner(
    ui: &mut egui::Ui,
    project: &Project,
    kind: &str,
    value: &Value,
    definition: &Value,
    null_toggle: bool,
    using_default: bool,
) -> Option<Value> {
    let mut current = value.clone();
    let mut modified = false;
    let nullable = null_toggle && definition["canBeNull"].as_bool().unwrap_or(false);
    if nullable {
        let mut is_null = current.is_null();
        if ui.checkbox(&mut is_null, "Use null").changed() {
            return Some(if is_null {
                Value::Null
            } else {
                default_value(kind)
            });
        }
        if is_null {
            return None;
        }
    }
    match kind {
        "Int" => {
            let mut v = current.as_i64().unwrap_or(0);
            let lo = definition["min"]
                .as_f64()
                .map(|n| (n + 0.5).floor() as i64)
                .unwrap_or(i32::MIN as i64);
            let hi = definition["max"]
                .as_f64()
                .map(|n| (n + 0.5).floor() as i64)
                .unwrap_or(i32::MAX as i64);
            let before = v;
            let cleared = std::cell::Cell::new(false);
            ui_commit::prepare_editor(ui.ctx(), ui.next_auto_id());
            let response = ui.add(
                egui::DragValue::new(&mut v)
                    .range(lo..=hi)
                    .update_while_editing(false)
                    .custom_parser(|s| {
                        cleared.set(s.trim().is_empty());
                        if cleared.get() {
                            None
                        } else {
                            Some(s.parse::<f64>().unwrap_or(0.).trunc())
                        }
                    }),
            );
            ui_commit::track_editor(ui.ctx(), &response);
            track_default_editor(ui, &response);
            modified = cleared.get() || v != before;
            current = if cleared.get() { Value::Null } else { json!(v) };
        }
        "Float" => {
            let mut v = current.as_f64().unwrap_or(0.0);
            let before = v;
            let cleared = std::cell::Cell::new(false);
            ui_commit::prepare_editor(ui.ctx(), ui.next_auto_id());
            let response = ui.add(
                egui::DragValue::new(&mut v)
                    .update_while_editing(false)
                    .custom_parser(|s| {
                        cleared.set(s.trim().is_empty());
                        if cleared.get() {
                            None
                        } else {
                            Some(s.parse::<f64>().unwrap_or(0.))
                        }
                    })
                    .range(
                        definition["min"].as_f64().unwrap_or(f64::MIN)
                            ..=definition["max"].as_f64().unwrap_or(f64::MAX),
                    )
                    .speed(0.1),
            );
            ui_commit::track_editor(ui.ctx(), &response);
            track_default_editor(ui, &response);
            modified = cleared.get() || v != before;
            current = if cleared.get() { Value::Null } else { json!(v) };
        }
        "Bool" => {
            let mut v = current.as_bool().unwrap_or(false);
            modified = ui.checkbox(&mut v, "").changed();
            current = json!(v);
        }
        "String" => {
            let decoded = current
                .as_str()
                .unwrap_or("")
                .replace("\\\\", "\\")
                .replace("\\n", "\n");
            if let Some(v) = buffered_text_kind(ui, "field-text", &decoded, false) {
                modified = true;
                current = json!(v);
            }
        }
        "Multilines" => {
            let decoded = current
                .as_str()
                .unwrap_or("")
                .replace("\\\\", "\\")
                .replace("\\n", "\n");
            if let Some(v) = text_editor::value_editor(
                ui,
                "field-text",
                &decoded,
                text(definition, "identifier", "Text"),
                text(definition, "textLanguageMode", ""),
                false,
            ) {
                modified = true;
                current = json!(v);
            }
        }
        "Color" => {
            let col = theme::color(current.as_str().unwrap_or("#FFFFFF"));
            let mut rgb = [col.r(), col.g(), col.b()];
            modified = super::panels::source_colors::color_editor_tag(
                ui,
                &mut rgb,
                Some(&format!("e_{}", text(definition, "identifier", ""))),
            )
            .changed();
            current = json!(format!("#{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2]));
        }
        "FilePath" => {
            let mut v = current.as_str().unwrap_or("").to_owned();
            let extensions = definition["acceptFileTypes"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .map(|s| s.trim_start_matches('.'))
                .collect::<Vec<_>>();
            ui.horizontal(|ui| {
                if let Some(next) = buffered_text(ui, "field-path", &v) {
                    v = next;
                    modified = true;
                }
                if super::panels::source_controls::small_button(ui, "Choose…").clicked()
                    && let Some(p) = pick_file(
                        ui,
                        &format!(
                            "field_{}_{}",
                            text(definition, "identifier", ""),
                            definition["uid"]
                        ),
                        &extensions,
                    )
                {
                    v = p.to_string_lossy().into_owned();
                    modified = true;
                }
                if !v.is_empty()
                    && let Ok(path) = project.make_absolute_file_path(&v)
                {
                    text_editor::external_file_editor(ui, &path);
                }
            });
            current = if v.is_empty() { Value::Null } else { json!(v) };
        }
        "Point" => {
            let mut x = current["cx"].as_i64().unwrap_or(0);
            let mut y = current["cy"].as_i64().unwrap_or(0);
            ui.horizontal(|ui| {
                ui.label("X");
                modified |= ui.add(egui::DragValue::new(&mut x)).changed();
                ui.label("Y");
                modified |= ui.add(egui::DragValue::new(&mut y)).changed();
            });
            current = json!({"cx":x,"cy":y});
        }
        "EntityRef" => {
            let mut selected = current["entityIid"].as_str().unwrap_or("").to_owned();
            let old = selected.clone();
            let mut options = vec![(String::new(), "None".to_owned(), Value::Null)];
            for key in project.level_keys() {
                if let Some(level) = project.level(key)
                    && let Some(layers) = level["layerInstances"].as_array()
                {
                    for layer in layers {
                        if let Some(entities) = layer["entityInstances"].as_array() {
                            for entity in entities {
                                if let Some(iid) = entity["iid"].as_str() {
                                    let world = key
                                        .world
                                        .and_then(|i| project.json["worlds"].get(i))
                                        .map(|w| w["iid"].clone())
                                        .unwrap_or_else(|| project.json["dummyWorldIid"].clone());
                                    options.push((iid.to_owned(),format!("{} / {} ({}, {})",text(level,"identifier","Level"),text(entity,"__identifier","Entity"),entity["px"][0],entity["px"][1]),json!({"entityIid":iid,"layerIid":layer["iid"],"levelIid":level["iid"],"worldIid":world})));
                                }
                            }
                        }
                    }
                }
            }
            let label = options
                .iter()
                .find(|(iid, _, _)| *iid == selected)
                .map(|(_, s, _)| s.as_str())
                .unwrap_or("None");
            egui::ComboBox::from_id_salt(ui.id().with("ref"))
                .selected_text(label)
                .show_ui(ui, |ui| {
                    for (iid, label, _) in &options {
                        ui.selectable_value(&mut selected, iid.clone(), label);
                    }
                });
            if selected != old {
                current = options
                    .into_iter()
                    .find(|(iid, _, _)| *iid == selected)
                    .map(|(_, _, v)| v)
                    .unwrap_or(Value::Null);
                modified = true;
            }
        }
        "Tile" => {
            let mut uid = current["tilesetUid"]
                .as_i64()
                .or_else(|| definition["tilesetUid"].as_i64())
                .unwrap_or(-1);
            let old_uid = uid;
            egui::ComboBox::from_id_salt(ui.id().with("tile-atlas"))
                .selected_text(
                    project
                        .tileset(uid)
                        .map(|d| text(d, "identifier", "Tileset"))
                        .unwrap_or("Select tileset"),
                )
                .show_ui(ui, |ui| {
                    if let Some(defs) = project.json["defs"]["tilesets"].as_array() {
                        for d in defs {
                            if let Some(id) = d["uid"].as_i64() {
                                ui.selectable_value(&mut uid, id, text(d, "identifier", "Tileset"));
                            }
                        }
                    }
                });
            let mut x = current["x"].as_i64().unwrap_or(0);
            let mut y = current["y"].as_i64().unwrap_or(0);
            let g = project
                .tileset(uid)
                .map(|d| number(d, "tileGridSize", 16.0) as i64)
                .unwrap_or(16);
            let mut w = current["w"].as_i64().unwrap_or(g);
            let mut h = current["h"].as_i64().unwrap_or(g);
            ui.horizontal(|ui| {
                for (label, v) in [("X", &mut x), ("Y", &mut y), ("W", &mut w), ("H", &mut h)] {
                    ui.label(label);
                    modified |= ui.add(egui::DragValue::new(v).range(0..=65536)).changed();
                }
            });
            current = json!({"tilesetUid":uid,"x":x,"y":y,"w":w,"h":h});
            modified |= uid != old_uid;
        }
        _ if kind.starts_with("LocalEnum.") || kind.starts_with("ExternEnum.") => {
            let name = kind.split_once('.').map(|(_, n)| n).unwrap_or(kind);
            let defs = project.json["defs"]["enums"]
                .as_array()
                .into_iter()
                .flatten()
                .chain(
                    project.json["defs"]["externalEnums"]
                        .as_array()
                        .into_iter()
                        .flatten(),
                );
            if let Some(def) = defs
                .into_iter()
                .find(|d| d["identifier"].as_str() == Some(name))
            {
                let mut v = current.as_str().unwrap_or("").to_owned();
                let old = v.clone();
                let uid = def["uid"].as_i64().unwrap_or(-1);
                let selected = project.enum_value(uid, &v);
                let display =
                    selected
                        .map(|value| text(value, "id", &v))
                        .unwrap_or(if v.is_empty() {
                            if definition["canBeNull"] == true {
                                "-- null --"
                            } else {
                                "[ Value required ]"
                            }
                        } else {
                            &v
                        });
                let label = if using_default && !definition["defaultOverride"].is_null() {
                    format!("{display} (default)")
                } else {
                    display.to_owned()
                };
                let mut reset = false;
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 2.;
                    if let Some(icon) = selected.and_then(|value| enum_icon(project, def, value)) {
                        ui.add(icon);
                    }
                    egui::ComboBox::from_id_salt(ui.id().with("enum"))
                        .width(ui.available_width().max(20.))
                        .truncate()
                        .selected_text(label)
                        .show_ui(ui, |ui| {
                            if definition["canBeNull"] == true || current.is_null() {
                                ui.selectable_value(&mut v, String::new(), "-- null --");
                            }
                            if !definition["defaultOverride"].is_null() {
                                let mut scalar = definition.clone();
                                scalar["isArray"] = json!(false);
                                if let Ok(default) = project.make_field_instance(&scalar) {
                                    let label = format!(
                                        "{} (default)",
                                        default["__value"].as_str().unwrap_or("null")
                                    );
                                    if ui.selectable_label(using_default, label).clicked() {
                                        reset = true;
                                    }
                                }
                            }
                            if let Some(values) = def["values"].as_array() {
                                for value in values {
                                    ui.horizontal(|ui| {
                                        if let Some(icon) = enum_icon(project, def, value) {
                                            ui.add(icon);
                                        }
                                        let id = text(value, "id", "");
                                        ui.selectable_value(&mut v, id.to_owned(), id);
                                    });
                                }
                            }
                        });
                });
                modified = reset || v != old;
                current = if reset || v.is_empty() {
                    Value::Null
                } else {
                    json!(v)
                };
            }
        }
        _ => {
            ui.label(current.to_string());
        }
    }
    modified.then_some(current)
}

fn default_value(kind: &str) -> Value {
    match kind {
        "Int" => json!(0),
        "Float" => json!(0.0),
        "Bool" => json!(false),
        "String" | "Multilines" | "FilePath" => json!(""),
        "Color" => json!("#000000"),
        "Point" => json!({"cx":0,"cy":0}),
        _ => Value::Null,
    }
}

fn enum_icon(project: &Project, definition: &Value, value: &Value) -> Option<egui::Image<'static>> {
    let uid = definition["iconTilesetUid"].as_i64()?;
    let tileset = project.tileset(uid)?;
    let tile = &value["tileRect"];
    if tile.is_null() {
        return None;
    }
    let image = if tileset["embedAtlas"].as_str().is_some() {
        egui::Image::new(egui::include_image!(
            "../../assets/embedAtlas/finalbossblues-icons_full_16.png"
        ))
    } else {
        let path = project
            .path
            .as_ref()?
            .parent()?
            .join(tileset["relPath"].as_str()?);
        egui::Image::new(format!("file://{}", path.to_string_lossy()))
    };
    let size = egui::vec2(
        number(tileset, "pxWid", 1.).max(1.),
        number(tileset, "pxHei", 1.).max(1.),
    );
    Some(
        image
            .uv(Rect::from_min_size(
                egui::pos2(
                    number(tile, "x", 0.) / size.x,
                    number(tile, "y", 0.) / size.y,
                ),
                egui::vec2(
                    number(tile, "w", 16.) / size.x,
                    number(tile, "h", 16.) / size.y,
                ),
            ))
            .fit_to_exact_size(egui::vec2(20., 20.))
            .texture_options(egui::TextureOptions::NEAREST),
    )
}

#[derive(Clone, Default)]
struct DefaultEditor {
    active: bool,
    focus: bool,
}
fn track_default_editor(ui: &egui::Ui, response: &egui::Response) {
    let id = ui.make_persistent_id("instance-default-editor");
    if let Some(mut state) = ui.data_mut(|data| data.get_temp::<DefaultEditor>(id)) {
        if state.focus {
            response.request_focus();
            state.focus = false;
        }
        if response.lost_focus() {
            state.active = false;
        }
        ui.data_mut(|data| data.insert_temp(id, state));
    }
}
pub(super) fn field_uses_default(field: &Value, index: usize) -> bool {
    field["realEditorValues"]
        .as_array()
        .and_then(|values| values.get(index))
        .unwrap_or(&Value::Null)
        .is_null()
}
fn instance_scalar_editor(
    ui: &mut egui::Ui,
    project: &Project,
    kind: &str,
    value: &Value,
    definition: &Value,
    using_default: bool,
) -> Option<Value> {
    let id = ui.make_persistent_id("instance-default-editor");
    if !using_default {
        ui.data_mut(|data| data.remove::<DefaultEditor>(id));
    }
    let state = ui
        .data_mut(|data| data.get_temp::<DefaultEditor>(id))
        .unwrap_or_default();
    if using_default && !state.active && matches!(kind, "Int" | "Float" | "String" | "Bool") {
        let width = ui.available_width().max(30.);
        let height = if kind == "Bool" { 28. } else { 33. };
        let (rect, response) = ui.allocate_exact_size(egui::vec2(width, height), Sense::click());
        ui.painter().rect_filled(rect, 0., theme::MEDIUM);
        egui::Image::new(egui::include_image!("../../assets/img/stripes.png"))
            .texture_options(egui::TextureOptions::NEAREST_REPEAT)
            .uv(Rect::from_min_size(egui::Pos2::ZERO, rect.size() / 8.))
            .paint_at(ui, rect);
        ui.painter().rect_stroke(
            rect.shrink(0.5),
            0.,
            Stroke::new(
                1_f32,
                if response.hovered() {
                    theme::ACCENT
                } else {
                    theme::LIGHT
                },
            ),
            StrokeKind::Inside,
        );
        let value_text = if value.is_null() {
            "null".to_owned()
        } else if matches!(kind, "Int" | "Float") {
            format!(
                "{}{}{}",
                text(definition, "editorTextPrefix", ""),
                value,
                text(definition, "editorTextSuffix", "")
            )
        } else if let Some(value) = value.as_str() {
            format!("\"{}\"", value.replace("\\\\", "\\").replace("\\n", "\n"))
        } else {
            value.to_string()
        };
        if kind == "Bool" {
            let check = Rect::from_min_size(rect.min + egui::vec2(2., 2.), egui::vec2(24., 24.));
            ui.painter()
                .rect_filled(check, 2., Color32::from_white_alpha(80));
            if value.as_bool() == Some(true) {
                ui.painter().text(
                    check.center(),
                    Align2::CENTER_CENTER,
                    "✓",
                    FontId::proportional(20.),
                    Color32::WHITE,
                );
            }
        } else {
            ui.painter()
                .with_clip_rect(ui.clip_rect().intersect(Rect::from_min_max(
                    rect.min + egui::vec2(8., 0.),
                    rect.max - egui::vec2(58., 0.),
                )))
                .text(
                    rect.left_center() + egui::vec2(8., 0.),
                    Align2::LEFT_CENTER,
                    value_text,
                    FontId::proportional(14.),
                    theme::TEXT.gamma_multiply(0.5),
                );
        }
        ui.painter().text(
            rect.right_center() - egui::vec2(8., 0.),
            Align2::RIGHT_CENTER,
            "Default",
            FontId::proportional(11.),
            Color32::from_rgb(115, 127, 157),
        );
        if response.clicked() {
            if kind == "Bool" {
                return Some(json!(!value.as_bool().unwrap_or(false)));
            }
            ui.data_mut(|data| {
                data.insert_temp(
                    id,
                    DefaultEditor {
                        active: true,
                        focus: true,
                    },
                )
            });
            ui.ctx().request_repaint();
        }
        return None;
    }
    let reset = !using_default
        && kind != "FilePath"
        && (definition["canBeNull"] == true || {
            let mut scalar = definition.clone();
            scalar["isArray"] = json!(false);
            project
                .make_field_instance(&scalar)
                .is_ok_and(|field| !field["__value"].is_null())
        });
    let mut changed = None;
    let mut next_state = None;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.;
        let width = (ui.available_width() - if reset { 24. } else { 0. }).max(20.);
        let state = ui.data_mut(|data| data.get_temp::<DefaultEditor>(id));
        let child = ui.allocate_ui_with_layout(
            egui::vec2(width, 24.),
            egui::Layout::top_down(egui::Align::Min),
            |ui| {
                ui.set_width(width);
                let child_id = ui.make_persistent_id("instance-default-editor");
                if let Some(state) = state {
                    ui.data_mut(|data| data.insert_temp(child_id, state));
                }
                let result =
                    scalar_editor_inner(ui, project, kind, value, definition, false, using_default);
                next_state = ui.data_mut(|data| data.get_temp::<DefaultEditor>(child_id));
                result
            },
        );
        changed = child.inner;
        if reset
            && ui
                .add_sized(
                    [22., 24.],
                    egui::Button::image(
                        egui::Image::new(egui::include_image!("../../assets/icons/reset.svg"))
                            .fit_to_exact_size(egui::vec2(20., 20.)),
                    )
                    .frame(false),
                )
                .on_hover_text("Reset to default")
                .clicked()
        {
            changed = Some(Value::Null);
        }
    });
    if let Some(state) = next_state {
        ui.data_mut(|data| data.insert_temp(id, state));
    }
    changed
}

pub(super) fn field_value_editor(
    ui: &mut egui::Ui,
    project: &Project,
    field: &Value,
    definition: &Value,
) -> Option<FieldEdit> {
    let kind = text(field, "__type", "String");
    let value = &field["__value"];
    if let Some(inner) = kind
        .strip_prefix("Array<")
        .and_then(|s| s.strip_suffix('>'))
    {
        let values = value.as_array().cloned().unwrap_or_default();
        let mut edit = None;
        let len = values.len();
        let sort_id = ui.id().with("array-sort");
        egui::Frame::new()
            .inner_margin(if values.is_empty() {
                egui::Margin::same(2)
            } else {
                egui::Margin {
                    left: 8,
                    right: 2,
                    top: 8,
                    bottom: 8,
                }
            })
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 2.;
                for (index, item) in values.iter().enumerate() {
                    ui.push_id(index, |ui| {
                        let row = ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 2.;
                            ui.dnd_drag_source(
                                ui.id().with("sort-field-entry"),
                                (sort_id, index),
                                |ui| {
                                    ui.add_sized(
                                        [12., 26.],
                                        egui::Label::new("≡").sense(Sense::drag()),
                                    )
                                    .on_hover_text("Drag to reorder");
                                },
                            );
                            let width = (ui.available_width() - 24.).max(20.);
                            ui.allocate_ui_with_layout(
                                egui::vec2(width, 29.),
                                egui::Layout::top_down(egui::Align::Min),
                                |ui| {
                                    ui.set_width(width);
                                    if let Some(next) = instance_scalar_editor(
                                        ui,
                                        project,
                                        inner,
                                        item,
                                        definition,
                                        field_uses_default(field, index),
                                    ) {
                                        edit = Some(ldtk_core::FieldArrayEdit::Set {
                                            index,
                                            value: next,
                                        });
                                    }
                                },
                            );
                            if super::panels::source_controls::small_button(ui, "×")
                                .on_hover_text("Remove array value")
                                .clicked()
                            {
                                edit = Some(ldtk_core::FieldArrayEdit::Remove { index });
                            }
                        });
                        if let Some(payload) =
                            row.response.dnd_release_payload::<(egui::Id, usize)>()
                            && payload.0 == sort_id
                            && payload.1 != index
                        {
                            edit = Some(ldtk_core::FieldArrayEdit::Move {
                                from: payload.1,
                                to: index,
                            });
                        }
                    });
                }
                let max = definition["arrayMaxLength"].as_u64().unwrap_or(u64::MAX);
                if (values.len() as u64) < max
                    && ui
                        .add_sized([ui.available_width(), 22.], egui::Button::new("+"))
                        .on_hover_text("Add value")
                        .clicked()
                {
                    edit = Some(ldtk_core::FieldArrayEdit::Insert { index: len });
                }
            });
        edit.map(FieldEdit::Array)
    } else {
        instance_scalar_editor(
            ui,
            project,
            kind,
            value,
            definition,
            field_uses_default(field, 0),
        )
        .map(FieldEdit::Value)
    }
}

pub(super) enum FieldEdit {
    Value(Value),
    Array(ldtk_core::FieldArrayEdit),
}
impl FieldEdit {
    pub(super) fn apply(
        self,
        project: &mut Project,
        key: LevelKey,
        entity: Option<&str>,
        uid: i64,
    ) -> ldtk_core::Result<()> {
        match self {
            Self::Value(value) => project.set_field_editor_value(key, entity, uid, value),
            Self::Array(edit) => project.edit_field_array(key, entity, uid, edit),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const LEVEL: LevelKey = LevelKey {
        world: None,
        level: 0,
    };

    fn frame(
        ctx: &egui::Context,
        p: &mut Project,
        uid: i64,
        events: Vec<egui::Event>,
    ) -> (egui::Rect, bool) {
        let def = p.json["defs"]["levelFields"][0].clone();
        let field = p.level(LEVEL).unwrap()["fieldInstances"][0].clone();
        let mut rect = egui::Rect::NOTHING;
        let mut changed = false;
        let output = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(600., 400.),
                )),
                events,
                ..Default::default()
            },
            |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    if let Some(v) = field_value_editor(ui, p, &field, &def) {
                        v.apply(p, LEVEL, None, uid).unwrap();
                        changed = true;
                    }
                    rect = ui.min_rect();
                });
            },
        );
        for shape in output.shapes {
            if let egui::Shape::Text(text) = shape.shape {
                let label = text.galley.text();
                if label == "+" || label == "×" {
                    ctx.data_mut(|d| {
                        d.insert_temp(egui::Id::new(label), text.pos + text.galley.size() * 0.5)
                    });
                }
            }
        }
        (rect, changed)
    }
    fn click(pos: egui::Pos2, pressed: bool) -> egui::Event {
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        }
    }
    fn key(key: Key) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }
    }
    fn focus(ctx: &egui::Context, p: &mut Project, uid: i64) {
        let (rect, _) = frame(ctx, p, uid, vec![]);
        let pos = rect.min + egui::vec2(10., 10.);
        frame(
            ctx,
            p,
            uid,
            vec![egui::Event::PointerMoved(pos), click(pos, true)],
        );
        frame(ctx, p, uid, vec![click(pos, false)]);
        frame(ctx, p, uid, vec![]);
        frame(ctx, p, uid, vec![]);
    }
    #[test]
    fn implicit_nonnullable_numeric_default_keeps_a_working_compact_reset() {
        let mut p = Project::new();
        let uid = p.add_field_definition(None, "Count", "Int", false).unwrap();
        p.set_field_editor_value(LEVEL, None, uid, json!(12))
            .unwrap();
        let ctx = egui::Context::default();
        let mut rect = Rect::NOTHING;
        let mut draw = |p: &mut Project, events| {
            let def = p.json["defs"]["levelFields"][0].clone();
            let field = p.level(LEVEL).unwrap()["fieldInstances"][0].clone();
            let _ = ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(320., 200.),
                    )),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        let shown = ui.scope(|ui| field_value_editor(ui, p, &field, &def));
                        rect = shown.response.rect;
                        if let Some(edit) = shown.inner {
                            edit.apply(p, LEVEL, None, uid).unwrap();
                        }
                    });
                },
            );
            rect
        };
        draw(&mut p, vec![]);
        let rect = draw(&mut p, vec![]);
        let pos = rect.right_center() - egui::vec2(11., 0.);
        draw(
            &mut p,
            vec![egui::Event::PointerMoved(pos), click(pos, true)],
        );
        draw(&mut p, vec![click(pos, false)]);
        assert_eq!(p.level(LEVEL).unwrap()["fieldInstances"][0]["__value"], 0);
        assert_eq!(
            p.level(LEVEL).unwrap()["fieldInstances"][0]["realEditorValues"],
            json!([null])
        );
    }
    #[test]
    fn enum_instance_display_and_icon_resolve_legacy_identifier_without_changing_wrappers() {
        let mut project = Project::load(resource_path("examples/original/Entities.ldtk")).unwrap();
        project.migrate_to_current().unwrap();
        let definition = project.json["defs"]["enums"]
            .as_array()
            .unwrap()
            .iter()
            .find(|definition| {
                project
                    .enum_value(definition["uid"].as_i64().unwrap(), "Heavy sword")
                    .is_some()
            })
            .unwrap();
        let value = project
            .enum_value(definition["uid"].as_i64().unwrap(), "Heavy sword")
            .unwrap();
        assert!(enum_icon(&project, definition, value).is_some());
        let field = json!({"__type":format!("LocalEnum.{}",definition["identifier"].as_str().unwrap()),"__value":"Heavy sword","realEditorValues":[{"id":"V_String","params":["Heavy sword"],"extension":17}]});
        let def = json!({"uid":1000,"__type":field["__type"],"type":{"id":"F_Enum","params":[definition["uid"]]},"isArray":false,"canBeNull":true});
        let before = field.clone();
        let ctx = egui::Context::default();
        let output = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                assert!(field_value_editor(ui, &project, &field, &def).is_none());
            });
        });
        assert!(output.shapes.iter().any(|shape|matches!(&shape.shape,egui::Shape::Text(text) if text.galley.job.text=="Heavy_sword")));
        assert_eq!(field, before);
    }
    #[test]
    fn dragging_array_rows_reorders_original_wrappers_and_default_slots() {
        let mut p = Project::new();
        let uid = p
            .add_field_definition(None, "Words", "String", true)
            .unwrap();
        p.set_field_default(uid, json!("default")).unwrap();
        p.set_field_editor_value(LEVEL, None, uid, json!([null, "first", "second"]))
            .unwrap();
        p.level_mut(LEVEL).unwrap()["fieldInstances"][0]["realEditorValues"][1]["extension"] =
            json!(17);
        let wrappers = p.level(LEVEL).unwrap()["fieldInstances"][0]["realEditorValues"]
            .as_array()
            .unwrap()
            .clone();
        let ctx = egui::Context::default();
        let mut time = 0.;
        let mut draw = |p: &mut Project, events| {
            time += 0.02;
            let field = p.level(LEVEL).unwrap()["fieldInstances"][0].clone();
            let def = p.json["defs"]["levelFields"][0].clone();
            ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(350., 400.),
                    )),
                    events,
                    time: Some(time),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        if let Some(edit) = field_value_editor(ui, p, &field, &def) {
                            edit.apply(p, LEVEL, None, uid).unwrap();
                        }
                    });
                },
            )
        };
        draw(&mut p, vec![]);
        let output = draw(&mut p, vec![]);
        fn handles(shape: &egui::Shape, positions: &mut Vec<egui::Pos2>) {
            match shape {
                egui::Shape::Text(text) if text.galley.job.text == "≡" => {
                    positions.push(text.pos + text.galley.size() * 0.5)
                }
                egui::Shape::Vec(shapes) => {
                    for shape in shapes {
                        handles(shape, positions);
                    }
                }
                _ => {}
            }
        }
        let mut positions = Vec::new();
        for shape in &output.shapes {
            handles(&shape.shape, &mut positions);
        }
        assert_eq!(positions.len(), 3);
        let start = positions[0];
        let end = positions[2] + egui::vec2(40., 0.);
        draw(
            &mut p,
            vec![egui::Event::PointerMoved(start), click(start, true)],
        );
        draw(
            &mut p,
            vec![egui::Event::PointerMoved(start + egui::vec2(8., 0.))],
        );
        draw(&mut p, vec![egui::Event::PointerMoved(end)]);
        draw(&mut p, vec![click(end, false)]);
        draw(&mut p, vec![]);
        assert_eq!(
            p.level(LEVEL).unwrap()["fieldInstances"][0]["realEditorValues"],
            json!([wrappers[1], wrappers[2], wrappers[0]])
        );
        assert_eq!(
            p.level(LEVEL).unwrap()["fieldInstances"][0]["__value"],
            json!(["first", "second", "default"])
        );
    }
    #[test]
    fn default_placeholder_includes_numeric_units_and_boolean_click_changes_only_that_field() {
        let mut p = Project::new();
        let uid = p.add_field_definition(None, "Life", "Int", false).unwrap();
        p.set_field_default(uid, json!(3)).unwrap();
        p.update_field_definition(uid, &json!({"editorTextSuffix":"hp"}))
            .unwrap();
        let ctx = egui::Context::default();
        let output = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                field_value_editor(
                    ui,
                    &p,
                    &p.level(LEVEL).unwrap()["fieldInstances"][0],
                    &p.json["defs"]["levelFields"][0],
                );
            });
        });
        assert!(output.shapes.iter().any(
            |shape| matches!(&shape.shape,egui::Shape::Text(text) if text.galley.job.text=="3hp")
        ));
        let mut p = Project::new();
        let uid = p
            .add_field_definition(None, "Awake", "Bool", false)
            .unwrap();
        p.set_field_default(uid, json!(true)).unwrap();
        let untouched = p
            .add_field_definition(None, "Untouched", "String", false)
            .unwrap();
        p.set_field_editor_value(LEVEL, None, untouched, json!("keep"))
            .unwrap();
        p.level_mut(LEVEL).unwrap()["fieldInstances"][1]["realEditorValues"][0]["extension"] =
            json!(17);
        let original = p.level(LEVEL).unwrap()["fieldInstances"][1].clone();
        frame(&ctx, &mut p, uid, vec![]);
        let (rect, _) = frame(&ctx, &mut p, uid, vec![]);
        let pos = rect.min + egui::vec2(10., 10.);
        frame(
            &ctx,
            &mut p,
            uid,
            vec![egui::Event::PointerMoved(pos), click(pos, true)],
        );
        frame(&ctx, &mut p, uid, vec![click(pos, false)]);
        assert_eq!(
            p.level(LEVEL).unwrap()["fieldInstances"][0]["__value"],
            false
        );
        assert!(!p.level(LEVEL).unwrap()["fieldInstances"][0]["realEditorValues"][0].is_null());
        assert_eq!(p.level(LEVEL).unwrap()["fieldInstances"][1], original);
    }
    #[test]
    fn regex_string_can_be_typed_character_by_character_before_commit() {
        let mut p = Project::new();
        let uid = p
            .add_field_definition(None, "Code", "String", false)
            .unwrap();
        p.update_field_definition(uid, &json!({"regex":"/^[0-9]{3}$/","canBeNull":false}))
            .unwrap();
        let ctx = egui::Context::default();
        focus(&ctx, &mut p, uid);
        for c in ["1", "2", "3"] {
            let (_, changed) = frame(&ctx, &mut p, uid, vec![egui::Event::Text(c.into())]);
            assert!(!changed, "typing must retain the draft until submission");
        }
        frame(&ctx, &mut p, uid, vec![key(Key::Enter)]);
        assert_eq!(
            p.level(LEVEL).unwrap()["fieldInstances"][0]["__value"],
            "123"
        );
    }
    #[test]
    fn string_spaces_and_multiline_newlines_survive_typing_until_blur() {
        for kind in ["String", "Multilines"] {
            let mut p = Project::new();
            let uid = p.add_field_definition(None, "Text", kind, false).unwrap();
            p.update_field_definition(uid, &json!({"canBeNull":false}))
                .unwrap();
            let ctx = egui::Context::default();
            focus(&ctx, &mut p, uid);
            for text in ["hello", " ", "world"] {
                assert!(!frame(&ctx, &mut p, uid, vec![egui::Event::Text(text.into())]).1);
            }
            frame(
                &ctx,
                &mut p,
                uid,
                vec![key(if kind == "Multilines" {
                    Key::Escape
                } else {
                    Key::Tab
                })],
            );
            assert_eq!(
                p.level(LEVEL).unwrap()["fieldInstances"][0]["__value"],
                "hello world"
            );
        }
    }
    #[test]
    fn array_add_and_remove_cross_minimum_length_and_use_definition_defaults() {
        let mut p = Project::new();
        let uid = p
            .add_field_definition(None, "Numbers", "Int", true)
            .unwrap();
        p.update_field_definition(uid, &json!({"arrayMinLength":3, "canBeNull":false}))
            .unwrap();
        p.set_field_default(uid, json!(7)).unwrap();
        let ctx = egui::Context::default();
        frame(&ctx, &mut p, uid, vec![]);
        for (label, expected) in [("+", json!([7])), ("×", json!([]))] {
            let pos = ctx
                .data_mut(|d| d.get_temp::<egui::Pos2>(egui::Id::new(label)))
                .unwrap();
            frame(
                &ctx,
                &mut p,
                uid,
                vec![egui::Event::PointerMoved(pos), click(pos, true)],
            );
            frame(&ctx, &mut p, uid, vec![click(pos, false)]);
            frame(&ctx, &mut p, uid, vec![]);
            assert_eq!(
                p.level(LEVEL).unwrap()["fieldInstances"][0]["__value"],
                expected
            );
        }
    }
    #[test]
    fn multiline_enter_inserts_newline_without_committing() {
        let mut p = Project::new();
        let uid = p
            .add_field_definition(None, "Text", "Multilines", false)
            .unwrap();
        p.update_field_definition(uid, &json!({"canBeNull":false}))
            .unwrap();
        let ctx = egui::Context::default();
        focus(&ctx, &mut p, uid);
        frame(&ctx, &mut p, uid, vec![egui::Event::Text("hello".into())]);
        assert!(!frame(&ctx, &mut p, uid, vec![key(Key::Enter)]).1);
        frame(&ctx, &mut p, uid, vec![egui::Event::Text("world".into())]);
        frame(&ctx, &mut p, uid, vec![key(Key::Escape)]);
        assert_eq!(
            p.level(LEVEL).unwrap()["fieldInstances"][0]["__value"],
            "hello\\nworld"
        );
    }
    #[test]
    fn clearing_numeric_text_uses_the_field_default() {
        for kind in ["Int", "Float"] {
            let mut p = Project::new();
            let uid = p.add_field_definition(None, "Number", kind, false).unwrap();
            p.set_field_default(uid, json!(7)).unwrap();
            p.set_field_value(LEVEL, None, uid, json!(12)).unwrap();
            p.update_field_definition(uid, &json!({"canBeNull":false}))
                .unwrap();
            let ctx = egui::Context::default();
            focus(&ctx, &mut p, uid);
            assert!(!frame(&ctx, &mut p, uid, vec![key(Key::Backspace)]).1);
            frame(&ctx, &mut p, uid, vec![key(Key::Enter)]);
            assert_eq!(
                p.level(LEVEL).unwrap()["fieldInstances"][0]["__value"].as_f64(),
                Some(7.)
            );
            assert_eq!(
                p.level(LEVEL).unwrap()["fieldInstances"][0]["realEditorValues"],
                json!([null])
            );
        }
    }
    #[test]
    fn numeric_keyboard_replacement_commits_only_on_enter() {
        for kind in ["Int", "Float"] {
            let mut p = Project::new();
            let uid = p.add_field_definition(None, "Number", kind, false).unwrap();
            p.set_field_value(LEVEL, None, uid, json!(12)).unwrap();
            p.update_field_definition(uid, &json!({"canBeNull":false}))
                .unwrap();
            let ctx = egui::Context::default();
            focus(&ctx, &mut p, uid);
            frame(&ctx, &mut p, uid, vec![egui::Event::Text("-".into())]);
            assert!(!frame(&ctx, &mut p, uid, vec![egui::Event::Text("35".into())]).1);
            frame(&ctx, &mut p, uid, vec![key(Key::Enter)]);
            assert_eq!(
                p.level(LEVEL).unwrap()["fieldInstances"][0]["__value"].as_f64(),
                Some(-35.)
            );
        }
    }
}
