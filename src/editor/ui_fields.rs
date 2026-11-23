use super::*;
use serde_json::json;

#[path = "enum_picker.rs"]
pub(super) mod enum_picker;

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
    enum_picker::prepare(app, ctx);
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
    let mut dialog = FileDialog::new();
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
    buffered_text_kind(ui, key, current, false, None, None)
}

pub(super) fn buffered_text_sized(
    ui: &mut egui::Ui,
    key: impl std::hash::Hash,
    current: &str,
    size: egui::Vec2,
) -> Option<String> {
    buffered_text_kind(ui, key, current, false, Some(size), None)
}

fn buffered_text_kind(
    ui: &mut egui::Ui,
    key: impl std::hash::Hash,
    current: &str,
    multiline: bool,
    size: Option<egui::Vec2>,
    hint: Option<&str>,
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
    let mut edit = edit.id(id);
    if let Some(hint) = hint {
        edit = edit.hint_text(hint);
    }
    let response = if let Some(size) = size {
        ui.add_sized(size, edit.desired_width(f32::INFINITY).min_size(size))
    } else {
        ui.add(edit.desired_width(240.0))
    };
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
    use super::panels::source_controls;
    let mut patch = serde_json::Map::new();
    egui::Grid::new(
        ui.id()
            .with(("property-form", properties.first().map(|p| p.0))),
    )
    .num_columns(2)
    .min_col_width(source_controls::PROPERTY_LABEL_WIDTH)
    .spacing([18.0, 8.0])
    .show(ui, |ui| {
        for Property(key, label, widget) in properties {
            source_controls::property_label(ui, label, key);
            if let Some(next) = property_value(ui, key, widget, &value[*key]) {
                patch.insert((*key).into(), next);
            }
            ui.end_row();
        }
    });
    Value::Object(patch)
}

pub(super) fn form_with_label_width(
    ui: &mut egui::Ui,
    value: &Value,
    properties: &[Property],
    label_width: f32,
) -> Value {
    use super::panels::source_controls;
    let mut patch = serde_json::Map::new();
    for Property(key, label, widget) in properties {
        let next = source_controls::form_row_wrapped(ui, label, key, label_width, |ui| {
            if matches!(widget, Widget::Bool) {
                let mut checked = value[*key].as_bool().unwrap_or(false);
                source_controls::checkbox(ui, &mut checked, "")
                    .changed()
                    .then(|| json!(checked))
            } else if matches!(widget, Widget::Text) {
                let size = egui::vec2(ui.available_width(), 33.);
                buffered_text_sized(ui, key, value[*key].as_str().unwrap_or(""), size)
                    .map(Value::String)
            } else {
                property_value(ui, key, widget, &value[*key])
            }
        });
        if let Some(next) = next {
            patch.insert((*key).into(), next);
        }
    }
    Value::Object(patch)
}

pub(super) fn property_value(
    ui: &mut egui::Ui,
    key: &str,
    widget: &Widget,
    current: &Value,
) -> Option<Value> {
    let mut current = current.clone();
    let changed = match widget {
        Widget::Text => {
            if let Some(v) = buffered_text(ui, key, current.as_str().unwrap_or("")) {
                current = json!(v);
                true
            } else {
                false
            }
        }
        Widget::Multiline => {
            if let Some(v) = buffered_text_kind(ui, key, current.as_str().unwrap_or(""), true, None, None)
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
                if key == "bgColor" { Some("bg") } else { None },
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
                    .map(|rgb| json!(format!("#{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2])))
                    .unwrap_or(Value::Null);
                true
            } else {
                false
            }
        }
        Widget::Choice(choices) => {
            let mut v = current.as_str().unwrap_or(choices[0]).to_owned();
            let old = v.clone();
            egui::ComboBox::from_id_salt(ui.id().with(key))
                .selected_text(locale::tr(ui, &v))
                .show_ui(ui, |ui| {
                    for option in *choices {
                        ui.selectable_value(&mut v, (*option).into(), locale::tr(ui, option));
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
                if let Some(next) = buffered_text(ui, key, &v) {
                    v = next;
                    c = true;
                }
                if super::panels::source_controls::small_button(ui, "…").clicked()
                    && let Some(p) = pick_file(
                        ui,
                        if key == "bgRelPath" || key == "relPath" {
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
    changed.then_some(current)
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

fn file_path_editor(
    ui: &mut egui::Ui,
    project: &Project,
    value: &Value,
    definition: &Value,
    using_default: bool,
) -> Option<Value> {
    let state_id = ui.make_persistent_id("instance-default-editor");
    let choose_new = ui.data_mut(|data| {
        data.get_temp::<DefaultEditor>(state_id)
            .is_some_and(|mut state| {
                let requested = std::mem::take(&mut state.focus);
                data.insert_temp(state_id, state);
                requested
            })
    });
    let path_text = value.as_str().filter(|path| !using_default && !path.is_empty());
    let path = path_text.and_then(|path| project.make_absolute_file_path(path).ok());
    let missing = path.as_ref().is_some_and(|path| !path.exists());
    let required = path_text.is_none() && definition["canBeNull"] != true;
    let danger = Color32::from_rgb(121, 17, 5);
    let height = 33.;
    let mut clear = false;
    let mut locate = false;
    let mut edit = false;
    let choose = ui
        .with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.set_min_height(height);
            ui.spacing_mut().item_spacing.x = 1.;
            ui.spacing_mut().button_padding = egui::vec2(2., 0.);
            ui.spacing_mut().interact_size.y = 29.;
            let action = |ui: &mut egui::Ui, icon, fill, tooltip: &str| {
                super::panels::source_controls::add_sized_button(
                    ui,
                    [23., 29.],
                    egui::Button::new(egui::Image::new(icon).fit_to_exact_size(Vec2::splat(18.)))
                        .fill(fill)
                        .stroke(Stroke::NONE)
                        .corner_radius(2.),
                )
                .on_hover_text(locale::tr(ui, tooltip))
                .clicked()
            };
            if path_text.is_some() {
                clear = action(
                    ui,
                    egui::include_image!("../../assets/icons/clear.svg"),
                    danger,
                    "Clear field value",
                );
                locate = action(
                    ui,
                    egui::include_image!("../../assets/icons/locate.svg"),
                    theme::LIGHT,
                    "Locate file",
                );
                edit = action(
                    ui,
                    egui::include_image!("../../assets/icons/edit.svg"),
                    theme::LIGHT,
                    "Edit file",
                );
            }
            let (rect, response) = ui.allocate_exact_size(
                egui::vec2(ui.available_width().max(20.), height),
                Sense::click(),
            );
            let invalid = missing || required;
            ui.painter().rect_filled(rect, 0., if invalid { danger } else { theme::MEDIUM });
            if path_text.is_none() {
                egui::Image::new(egui::include_image!("../../assets/img/stripes.png"))
                    .texture_options(egui::TextureOptions::NEAREST_REPEAT)
                    .uv(Rect::from_min_size(egui::Pos2::ZERO, rect.size() / 8.))
                    .paint_at(ui, rect);
            }
            let highlight = invalid || response.hovered() || response.has_focus();
            ui.painter().rect_stroke(
                rect,
                0.,
                Stroke::new(1_f32, if highlight { theme::ACCENT } else { theme::LIGHT }),
                StrokeKind::Inside,
            );
            let filename = path_text
                .and_then(|path| path.rsplit(['/', '\\']).next())
                .unwrap_or("null");
            let muted = Color32::from_rgb(115, 127, 157);
            ui.painter()
                .with_clip_rect(rect.shrink2(egui::vec2(8., 1.)))
                .text(
                    rect.left_center() + egui::vec2(9., 0.),
                    Align2::LEFT_CENTER,
                    filename,
                    FontId::proportional(14.),
                    if invalid { theme::ACCENT } else { muted },
                );
            if path_text.is_none() {
                ui.painter().text(
                    rect.right_center() - egui::vec2(8., 0.),
                    Align2::RIGHT_CENTER,
                    locale::tr(ui, if required { "Required!" } else { "Default" }),
                    FontId::proportional(11.),
                    if required { theme::ACCENT } else { muted },
                );
            }
            response.widget_info(|| {
                egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), filename)
            });
            let response = response
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .on_hover_text(path_text.unwrap_or_else(|| locale::tr(ui, "Choose…")));
            if response.clicked() {
                response.surrender_focus();
                return true;
            }
            false
        })
        .inner;
    if clear {
        return Some(Value::Null);
    }
    if let Some(path) = &path {
        let error_id = ui.make_persistent_id(("locate-field-file-error", path));
        if locate {
            let target = if missing {
                path.parent().unwrap_or(path)
            } else {
                path
            };
            let error = if target.exists() {
                home_files::locate(target, !missing)
                    .err()
                    .map(|error| error.to_string())
            } else {
                Some(locale::tr(ui, "File not found!").to_owned())
            };
            ui.data_mut(|data| {
                if let Some(error) = error {
                    data.insert_temp(error_id, error);
                } else {
                    data.remove::<String>(error_id);
                }
            });
        }
        if let Some(error) = ui.data(|data| data.get_temp::<String>(error_id)) {
            ui.colored_label(Color32::LIGHT_RED, error);
        }
        text_editor::external_file_editor(ui, path, edit);
    }
    if choose || choose_new {
        let extensions = definition["acceptFileTypes"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(|s| s.trim_start_matches('.'))
            .collect::<Vec<_>>();
        let id = format!(
            "field_{}_{}",
            text(definition, "identifier", ""),
            definition["uid"]
        );
        return pick_file(ui, &id, &extensions).map(|path| json!(path.to_string_lossy()));
    }
    None
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
            prepare_number_editor(ui);
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
            prepare_number_editor(ui);
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
            modified = panels::source_controls::checkbox(ui, &mut v, "").changed();
            current = json!(v);
        }
        "String" => {
            let decoded = current
                .as_str()
                .unwrap_or("")
                .replace("\\\\", "\\")
                .replace("\\n", "\n");
            let (initial, hint) = if using_default {
                let hint = if current.is_null() {
                    locale::tr(ui, "(null)")
                } else if decoded.is_empty() {
                    locale::tr(ui, "(empty string)")
                } else {
                    &decoded
                };
                ("", Some(hint))
            } else {
                (decoded.as_str(), None)
            };
            let size = (!null_toggle).then(|| egui::vec2(ui.available_width().max(20.), 32.));
            if let Some(v) = buffered_text_kind(ui, "field-text", initial, false, size, hint) {
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
        "FilePath" => return file_path_editor(ui, project, &current, definition, using_default),
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
            let mut options = Vec::new();
            for key in project.level_keys() {
                if let Some(level) = project.level(key)
                    && let Some(layers) = level["layerInstances"].as_array()
                {
                    for layer in layers {
                        for entity in layer["entityInstances"].as_array().into_iter().flatten() {
                            if let Some(iid) = entity["iid"].as_str() {
                                options.push((iid, entity, layer, level, key.world));
                            }
                        }
                    }
                }
            }
            let label_for = |entity: &Value, level: &Value| {
                format!(
                    "{} / {} ({}, {})",
                    text(level, "identifier", "Level"),
                    text(entity, "__identifier", "Entity"),
                    entity["px"][0],
                    entity["px"][1]
                )
            };
            let label = options
                .iter()
                .find(|(iid, _, _, _, _)| *iid == selected)
                .map(|(_, entity, _, level, _)| label_for(entity, level))
                .unwrap_or_else(|| "None".to_owned());
            egui::ComboBox::from_id_salt(ui.id().with("ref"))
                .selected_text(label)
                .show_ui(ui, |ui| {
                    let count = options.len() + 1;
                    let spacing = ui.spacing().item_spacing.y;
                    let row_height = ui.spacing().interact_size.y.max(
                        ui.text_style_height(&egui::TextStyle::Button)
                            + 2. * ui.spacing().button_padding.y,
                    ) + spacing;
                    let top = ui.max_rect().top();
                    let clip = ui.clip_rect();
                    let first =
                        (((clip.top() - top) / row_height).floor().max(0.) as usize).min(count);
                    let last = (((clip.bottom() - top) / row_height).ceil().max(0.) as usize + 1)
                        .min(count);
                    ui.set_height((row_height * count as f32 - spacing).max(0.));
                    let rect = Rect::from_x_y_ranges(
                        ui.max_rect().x_range(),
                        (top + first as f32 * row_height)..=(top + last as f32 * row_height),
                    );
                    ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
                        ui.skip_ahead_auto_ids(first);
                        for index in first..last {
                            if index == 0 {
                                ui.selectable_value(&mut selected, String::new(), "None");
                            } else {
                                let (iid, entity, _, level, _) = options[index - 1];
                                ui.selectable_value(
                                    &mut selected,
                                    iid.to_owned(),
                                    label_for(entity, level),
                                );
                            }
                        }
                    });
                });
            if selected != old {
                current = options.into_iter().find(|(iid, _, _, _, _)| *iid == selected).map(|(iid, _, layer, level, world)| {
                    let world = world.and_then(|i| project.json["worlds"].get(i)).map(|w| w["iid"].clone()).unwrap_or_else(|| project.json["dummyWorldIid"].clone());
                    json!({"entityIid":iid,"layerIid":layer["iid"],"levelIid":level["iid"],"worldIid":world})
                }).unwrap_or(Value::Null);
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
                return enum_picker::show(ui, project, def, definition, &current, using_default);
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

pub(super) fn reference_target<'a>(
    project: &'a Project,
    reference: &Value,
) -> Option<(LevelKey, &'a Value, &'a Value)> {
    let iid = reference["entityIid"].as_str()?;
    project.level_keys().into_iter().find_map(|key| {
        let level = project.level(key)?;
        let entity = canvas::values(&level["layerInstances"])
            .iter()
            .flat_map(|layer| canvas::values(&layer["entityInstances"]))
            .find(|entity| entity["iid"] == iid)?;
        Some((key, level, entity))
    })
}

pub(super) fn reference_preview<'a>(
    project: &'a Project,
    entity: &Value,
) -> Option<(&'a Value, Option<Value>)> {
    let def = project.json["defs"]["entities"]
        .as_array()?
        .iter()
        .find(|def| def["uid"] == entity["defUid"])?;
    Some((def, canvas::entity_field_tile(project, entity, def)))
}

#[derive(Clone, Default)]
struct DefaultEditor {
    active: bool,
    focus: bool,
}
fn prepare_number_editor(ui: &egui::Ui) {
    let id = ui.next_auto_id();
    let default = ui.make_persistent_id("instance-default-editor");
    if ui
        .data(|data| data.get_temp::<DefaultEditor>(default))
        .is_some_and(|state| state.focus)
    {
        // DragValue selects the old number when it observes the focus change.
        ui.memory_mut(|memory| memory.request_focus(id));
    }
    ui_commit::prepare_editor(ui.ctx(), id);
}
fn track_default_editor(ui: &egui::Ui, response: &egui::Response) {
    let id = ui.make_persistent_id("instance-default-editor");
    if let Some(mut state) = ui.data_mut(|data| data.get_temp::<DefaultEditor>(id)) {
        if state.focus {
            response.request_focus();
            response.scroll_to_me(None);
            state.focus = false;
            ui.ctx().request_repaint();
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
        let height = if kind == "Bool" { 32. } else { 33. };
        let (rect, response) = ui.allocate_exact_size(egui::vec2(width, height), Sense::click());
        let hovered = response.contains_pointer();
        let opacity = if hovered { 1. } else { 0.35 };
        let mut painter = ui.painter().clone();
        painter.multiply_opacity(opacity);
        ui.painter().rect_filled(rect, 0., theme::MEDIUM);
        egui::Image::new(egui::include_image!("../../assets/img/stripes.png"))
            .texture_options(egui::TextureOptions::NEAREST_REPEAT)
            .tint(Color32::WHITE.gamma_multiply(opacity))
            .uv(Rect::from_min_size(egui::Pos2::ZERO, rect.size() / 8.))
            .paint_at(ui, rect);
        let border = rect.shrink(0.5);
        painter.extend(egui::Shape::dashed_line(
            &[
                border.left_top(),
                border.right_top(),
                border.right_bottom(),
                border.left_bottom(),
                border.left_top(),
            ],
            Stroke::new(1_f32, if hovered { theme::ACCENT } else { theme::LIGHT }),
            2.,
            1.,
        ));
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
        let toggled = if kind == "Bool" {
            let check = Rect::from_min_size(rect.min + egui::vec2(5., 4.), egui::vec2(24., 24.));
            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(check));
            child.multiply_opacity(opacity * opacity);
            let mut checked = value.as_bool().unwrap_or(false);
            panels::source_controls::checkbox(&mut child, &mut checked, "").changed()
        } else {
            painter
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
            false
        };
        let label = RichText::new(locale::tr(ui, "Default"))
            .italics()
            .color(Color32::from_rgb(115, 127, 157));
        let label =
            egui::WidgetText::from(label).into_galley(ui, None, 58., FontId::proportional(10.));
        painter.galley(
            rect.right_center() - egui::vec2(8. + label.size().x, label.size().y * 0.5),
            label,
            theme::TEXT,
        );
        if response.clicked() || toggled {
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
    let scalar_checkbox = kind == "Bool" && definition["isArray"] != true;
    let draw = |ui: &mut egui::Ui| {
        let checkbox = kind == "Bool";
        let reset_gap = if scalar_checkbox {
            13.
        } else if checkbox || definition["isArray"] == true {
            10.
        } else {
            2.
        };
        ui.spacing_mut().item_spacing.x = reset_gap;
        let reserved = if reset { 22. + reset_gap } else { 0. };
        let width = if checkbox {
            ui.add_space(if scalar_checkbox { 7. } else { 10. });
            24.
        } else {
            (ui.available_width() - reserved).max(20.)
        };
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
                .on_hover_text(locale::tr(ui, "Reset to default"))
                .clicked()
        {
            changed = Some(Value::Null);
        }
    };
    if scalar_checkbox {
        egui::Frame::new()
            .inner_margin(egui::Margin::symmetric(0, 6))
            .show(ui, |ui| ui.horizontal(draw));
    } else {
        ui.horizontal(draw);
    }
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
        let append_focus_id = ui.id().with("appended-array-item");
        let activate = ui.data_mut(|data| data.remove_temp::<usize>(append_focus_id));
        let group = field_tools::array_field_frame(values.is_empty())
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.;
                let moved = field_tools::sortable_array_field_rows(ui, len, |ui, index| {
                    let item = &values[index];
                    if inner == "EntityRef"
                        && let Some((_, _, entity)) = reference_target(project, item)
                        && let Some((def, tile)) = reference_preview(project, entity)
                    {
                        let tile = tile.as_ref().unwrap_or(if number(&def["uiTileRect"], "w", 0.) > 0. {
                            &def["uiTileRect"]
                        } else {
                            &def["tileRect"]
                        });
                        if let Some(icon) = tile["tilesetUid"].as_i64().and_then(|uid| {
                            enum_icon(
                                project,
                                &json!({"iconTilesetUid": uid}),
                                &json!({"tileRect": tile}),
                            )
                        }) {
                            ui.add(icon);
                        } else {
                            let (rect, _) = ui
                                .allocate_exact_size(egui::vec2(20., 20.), Sense::hover());
                            ui.painter().rect_filled(
                                rect,
                                2.,
                                theme::color(text(def, "color", "#ffcc00")),
                            );
                        }
                    }
                    let width = (ui.available_width() - 24.).max(20.);
                    ui.allocate_ui_with_layout(
                        egui::vec2(width, 33.),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            ui.set_width(width);
                            if activate == Some(index) && field_uses_default(field, index) {
                                let id = ui.make_persistent_id("instance-default-editor");
                                ui.data_mut(|data| {
                                    data.insert_temp(
                                        id,
                                        DefaultEditor {
                                            active: true,
                                            focus: true,
                                        },
                                    )
                                });
                            }
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
                    if field_tools::array_field_remove(ui).clicked() {
                        edit = Some(ldtk_core::FieldArrayEdit::Remove { index });
                    }
                });
                if let Some(moved) = moved {
                    edit = Some(moved);
                }
                let max = definition["arrayMaxLength"].as_u64().unwrap_or(u64::MAX);
                if (len as u64) < max {
                    if len > 0 {
                        ui.add_space(2.);
                    }
                    if field_tools::array_field_add(ui, "Add a new value").clicked() {
                        if matches!(inner, "String" | "Int" | "Float" | "FilePath") {
                            ui.data_mut(|data| data.insert_temp(append_focus_id, len));
                            ui.ctx().request_repaint();
                        }
                        edit = Some(ldtk_core::FieldArrayEdit::Insert { index: len });
                    }
                }
            });
        if !values.is_empty() {
            field_tools::array_field_border(ui, group.response.rect);
        }
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
                    let shown = ui.scope(|ui| field_value_editor(ui, p, &field, &def));
                    if let Some(v) = shown.inner {
                        v.apply(p, LEVEL, None, uid).unwrap();
                        changed = true;
                    }
                    rect = shown.response.rect;
                });
            },
        );
        for shape in output.shapes {
            let button = match shape.shape {
                egui::Shape::Rect(rect) if rect.fill == Color32::from_rgb(49, 55, 66) => {
                    Some(("+", rect.rect.center()))
                }
                egui::Shape::Rect(image)
                    if image.brush.is_some()
                        && image.rect.size() == Vec2::splat(20.)
                        && image.rect.right() >= rect.right() - 3. =>
                {
                    Some(("remove", image.rect.center()))
                }
                _ => None,
            };
            if let Some((label, point)) = button {
                ctx.data_mut(|d| d.insert_temp(egui::Id::new(label), point));
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
    fn preview_154_reference_array_preview_distinguishes_instance_tile_from_definition_tile() {
        let mut p = Project::new();
        p.add_layer("Actors", "Entities", 16).unwrap();
        let def = p.add_entity_def("Actor", 16, 16).unwrap();
        let atlas = p.ensure_embedded_icons().unwrap();
        let field = p
            .add_field_definition(Some(def), "Icon", "Tile", false)
            .unwrap();
        p.update_field_definition(field, &json!({"editorDisplayMode":"EntityTile","tilesetUid":atlas}))
            .unwrap();
        p.add_entity(LEVEL, 0, def, 16, 16).unwrap();
        let iid = p.layer(LEVEL, 0).unwrap()["entityInstances"][0]["iid"].clone();
        p.json["defs"]["entities"][0]["uiTileRect"] =
            json!({"tilesetUid":atlas,"x":16,"y":0,"w":16,"h":16});
        let tile = json!({"tilesetUid":atlas,"x":32,"y":0,"w":16,"h":16});
        p.json["defs"]["entities"][0]["tileRect"] = tile.clone();
        let reference = json!({"entityIid":iid});
        let override_tile = |project: &Project| {
            let (_, _, entity) = reference_target(project, &reference).unwrap();
            reference_preview(project, entity).unwrap().1
        };
        assert_eq!(override_tile(&p), None);
        p.set_field_editor_value(LEVEL, iid.as_str(), field, tile.clone())
            .unwrap();
        assert_eq!(override_tile(&p), Some(tile));
        assert!(reference_target(&p, &json!({"entityIid":"missing"})).is_none());
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
        egui_extras::install_image_loaders(&ctx);
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
                egui::Shape::Rect(image)
                    if image.brush.is_some()
                        && image.rect.size() == Vec2::splat(18.)
                        && image.rect.left() < 30. =>
                {
                    positions.push(image.rect.center());
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
        let first_top = |output: &egui::FullOutput| {
            output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) if text.galley.text() == "first" => Some(text.pos.y),
                    _ => None,
                })
                .unwrap()
        };
        let before = first_top(&output);
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
        assert_eq!(first_top(&draw(&mut p, vec![])), before);
        let halfway = first_top(&draw(&mut p, vec![]));
        let mut settled = halfway;
        for _ in 0..5 {
            settled = first_top(&draw(&mut p, vec![]));
        }
        assert!(settled < halfway && halfway < before);
        assert_eq!(
            p.level(LEVEL).unwrap()["fieldInstances"][0]["realEditorValues"],
            json!(wrappers)
        );
        let outside = egui::pos2(end.x, 280.);
        draw(&mut p, vec![egui::Event::PointerMoved(outside)]);
        draw(&mut p, vec![click(outside, false)]);
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
        assert_eq!(rect.height(), 32.);
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

        let (rect, _) = frame(&ctx, &mut p, uid, vec![]);
        assert_eq!(rect.height(), 36.);
        let reset = rect.min + egui::vec2(55., 18.);
        frame(
            &ctx,
            &mut p,
            uid,
            vec![egui::Event::PointerMoved(reset), click(reset, true)],
        );
        frame(&ctx, &mut p, uid, vec![click(reset, false)]);
        let field = &p.level(LEVEL).unwrap()["fieldInstances"][0];
        assert_eq!(field["__value"], true);
        assert!(field_uses_default(field, 0));

        let (rect, _) = frame(&ctx, &mut p, uid, vec![]);
        let default_label = rect.right_center() - egui::vec2(16., 0.);
        frame(
            &ctx,
            &mut p,
            uid,
            vec![
                egui::Event::PointerMoved(default_label),
                click(default_label, true),
            ],
        );
        frame(&ctx, &mut p, uid, vec![click(default_label, false)]);
        let field = &p.level(LEVEL).unwrap()["fieldInstances"][0];
        assert_eq!(field["__value"], false);
        assert!(!field_uses_default(field, 0));
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
            theme::configure(&ctx);
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
        egui_extras::install_image_loaders(&ctx);
        frame(&ctx, &mut p, uid, vec![]);
        for (label, expected) in [("+", json!([7])), ("remove", json!([]))] {
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
    fn appended_text_and_number_slots_accept_typing_without_an_extra_click() {
        for (kind, existing, typed, expected) in [
            ("String", json!("old"), "fresh", json!("fresh")),
            ("Int", json!(1), "37", json!(37)),
            ("Float", json!(1.5), "27.5", json!(27.5)),
        ] {
            let mut project = Project::new();
            let field = project
                .add_field_definition(None, "Values", kind, true)
                .unwrap();
            project
                .set_field_editor_value(LEVEL, None, field, json!([existing]))
                .unwrap();
            let ctx = egui::Context::default();
            frame(&ctx, &mut project, field, vec![]);
            let add = ctx
                .data_mut(|d| d.get_temp::<egui::Pos2>(egui::Id::new("+")))
                .unwrap();
            frame(
                &ctx,
                &mut project,
                field,
                vec![egui::Event::PointerMoved(add), click(add, true)],
            );
            frame(&ctx, &mut project, field, vec![click(add, false)]);
            frame(&ctx, &mut project, field, vec![]);
            frame(&ctx, &mut project, field, vec![]);
            assert!(
                project.level(LEVEL).unwrap()["fieldInstances"][0]["realEditorValues"][1].is_null()
            );
            frame(
                &ctx,
                &mut project,
                field,
                vec![egui::Event::Text(typed.to_owned())],
            );
            frame(&ctx, &mut project, field, vec![key(Key::Enter)]);
            assert_eq!(
                project.level(LEVEL).unwrap()["fieldInstances"][0]["__value"],
                json!([existing, expected]),
                "{kind}"
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
        theme::configure(&ctx);
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
    #[test]
    fn preview_154_large_reference_menu_scrolls_and_selects_last_entity() {
        let mut project = Project::new();
        let entities = (0..1000).map(|index| json!({"iid":format!("entity-{index}"),"__identifier":format!("Actor{index}"),"px":[0,0]})).collect::<Vec<_>>();
        project.json["levels"][0]["layerInstances"] =
            json!([{"iid":"layer","entityInstances":entities}]);
        let ctx = egui::Context::default();
        let mut style = (*ctx.style()).clone();
        style
            .text_styles
            .insert(egui::TextStyle::Button, egui::FontId::proportional(22.));
        ctx.set_style(style);
        let chosen = std::cell::RefCell::new(None);
        let draw = |events| {
            ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(900., 600.),
                    )),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        if let Some(value) = scalar_editor(
                            ui,
                            &project,
                            "EntityRef",
                            &Value::Null,
                            &json!({"canBeNull":false}),
                        ) {
                            *chosen.borrow_mut() = Some(value);
                        }
                    });
                },
            )
        };
        let find = |output: &egui::FullOutput, text: &str| {
            output.shapes.iter().find_map(|shape| match &shape.shape {
                egui::Shape::Text(label) if label.galley.text().contains(text) => {
                    Some(label.pos + label.galley.size() * 0.5)
                }
                _ => None,
            })
        };
        let initial = draw(vec![]);
        let button = find(&initial, "None").unwrap();
        draw(vec![egui::Event::PointerMoved(button), click(button, true)]);
        draw(vec![click(button, false)]);
        let menu = draw(vec![]);
        let first = find(&menu, "Actor0").expect("first option visible");
        draw(vec![
            egui::Event::PointerMoved(first),
            egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: egui::vec2(0., -100000.),
                modifiers: egui::Modifiers::NONE,
            },
        ]);
        let mut last = None;
        for _ in 0..5 {
            last = last.or_else(|| find(&draw(vec![]), "Actor999"));
        }
        let last = last.expect("last option visible after scrolling");
        draw(vec![egui::Event::PointerMoved(last), click(last, true)]);
        draw(vec![click(last, false)]);
        let value = chosen.into_inner().expect("last option selected");
        assert_eq!(value["entityIid"], "entity-999");
        assert_eq!(value["layerIid"], "layer");
        assert_eq!(value["levelIid"], project.json["levels"][0]["iid"]);
    }

    #[test]
    #[ignore = "controlled large entity reference menu benchmark"]
    fn benchmark_large_entity_reference_menu() {
        let mut project = Project::new();
        let entities = (0..5000).map(|index| json!({"iid":format!("entity-{index}"),"__identifier":format!("Actor{index}"),"px":[0,0]})).collect::<Vec<_>>();
        project.json["levels"][0]["layerInstances"] =
            json!([{"iid":"layer","entityInstances":entities}]);
        let ctx = egui::Context::default();
        let draw = |events| {
            ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(900., 600.),
                    )),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        scalar_editor(
                            ui,
                            &project,
                            "EntityRef",
                            &Value::Null,
                            &json!({"canBeNull":false}),
                        );
                    });
                },
            )
        };
        let initial = draw(vec![]);
        let pos = initial
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text() == "None" => {
                    Some(text.pos + text.galley.size() * 0.5)
                }
                _ => None,
            })
            .unwrap();
        draw(vec![egui::Event::PointerMoved(pos), click(pos, true)]);
        draw(vec![click(pos, false)]);
        let started = std::time::Instant::now();
        let mut seen = false;
        for _ in 0..5 {
            let output = draw(vec![]);
            seen |= output.shapes.iter().any(|shape| matches!(&shape.shape, egui::Shape::Text(text) if text.galley.text().contains("Actor0")));
        }
        assert!(seen, "entity reference menu did not open");
        eprintln!(
            "5000 entity options, five open-menu frames: {:?}",
            started.elapsed()
        );
    }
    #[test]
    fn preview_154_large_enum_menu_keeps_special_options_and_selects_last_value() {
        let mut project = Project::new();
        let values = (0..1000).map(|index| json!({"id":format!("Entry{index}"),"tileRect": if index % 2 == 0 { json!({"x":0,"y":0,"w":16,"h":16}) } else { Value::Null }})).collect::<Vec<_>>();
        project.json["defs"]["tilesets"] =
            json!([{"uid":2,"embedAtlas":"LdtkIcons","pxWid":256,"pxHei":256}]);
        project.json["defs"]["enums"] =
            json!([{"uid":1,"identifier":"Big","iconTilesetUid":2,"values":values}]);
        let definition = json!({"uid":100,"__type":"LocalEnum.Big","type":{"id":"F_Enum","params":[1]},"isArray":false,"canBeNull":true,"defaultOverride":{"id":"V_String","params":["Entry0"]}});
        let ctx = egui::Context::default();
        let chosen = std::cell::RefCell::new(None);
        let draw = |events| {
            ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(900., 600.),
                    )),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        if let Some(value) = scalar_editor(
                            ui,
                            &project,
                            "LocalEnum.Big",
                            &json!("Entry0"),
                            &definition,
                        ) {
                            *chosen.borrow_mut() = Some(value);
                        }
                    });
                },
            )
        };
        let find = |output: &egui::FullOutput, text: &str| {
            output.shapes.iter().find_map(|shape| match &shape.shape {
                egui::Shape::Text(label) if label.galley.text() == text => {
                    Some(label.pos + label.galley.size() * 0.5)
                }
                _ => None,
            })
        };
        let initial = draw(vec![]);
        let button = find(&initial, "Entry0").unwrap();
        draw(vec![egui::Event::PointerMoved(button), click(button, true)]);
        draw(vec![click(button, false)]);
        let menu = draw(vec![]);
        assert!(find(&menu, "Search").is_some());
        assert!(find(&menu, "-- null --").is_some());
        assert!(find(&menu, "Entry0 (default)").is_some());
        let first = find(&menu, "Entry1").expect("first enum options visible");
        draw(vec![
            egui::Event::PointerMoved(first),
            egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: egui::vec2(0., -100000.),
                modifiers: egui::Modifiers::NONE,
            },
        ]);
        let mut last = None;
        for _ in 0..5 {
            last = last.or_else(|| find(&draw(vec![]), "Entry999"));
        }
        let last = last.expect("last enum option visible after scrolling");
        draw(vec![egui::Event::PointerMoved(last), click(last, true)]);
        draw(vec![click(last, false)]);
        assert_eq!(*chosen.borrow(), Some(json!("Entry999")));
        chosen.borrow_mut().take();
        draw(vec![]);

        draw(vec![egui::Event::PointerMoved(button), click(button, true)]);
        draw(vec![click(button, false)]);
        let menu = draw(vec![]);
        assert!(find(&menu, "Search").is_some());
        let filtered = draw(vec![egui::Event::Text("e999".into())]);
        assert!(find(&filtered, "Entry999").is_some());
        assert!(find(&filtered, "Entry998").is_none());
        draw(vec![egui::Event::Key {
            key: Key::Enter,
            physical_key: Some(Key::Enter),
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        }]);
        assert_eq!(chosen.into_inner(), Some(json!("Entry999")));
    }

    #[test]
    #[ignore = "controlled large enum menu benchmark"]
    fn benchmark_large_enum_menu() {
        let mut project = Project::new();
        let values = (0..5000)
            .map(|index| json!({"id":format!("Entry{index}")}))
            .collect::<Vec<_>>();
        project.json["defs"]["enums"] = json!([{"uid":1,"identifier":"Big","values":values}]);
        let ctx = egui::Context::default();
        let draw = |events| {
            ctx.run(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(900., 600.),
                    )),
                    events,
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        scalar_editor(
                            ui,
                            &project,
                            "LocalEnum.Big",
                            &json!("Entry0"),
                            &json!({"canBeNull":false}),
                        );
                    });
                },
            )
        };
        let initial = draw(vec![]);
        let pos = initial
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text() == "Entry0" => {
                    Some(text.pos + text.galley.size() * 0.5)
                }
                _ => None,
            })
            .unwrap();
        draw(vec![egui::Event::PointerMoved(pos), click(pos, true)]);
        draw(vec![click(pos, false)]);
        let started = std::time::Instant::now();
        let mut seen = false;
        for _ in 0..5 {
            let output = draw(vec![]);
            seen |= output.shapes.iter().any(|shape| matches!(&shape.shape, egui::Shape::Text(text) if text.galley.text()=="Entry1"));
        }
        assert!(seen, "enum menu did not open");
        eprintln!(
            "5000 enum options, five open-menu frames: {:?}",
            started.elapsed()
        );
    }
}
