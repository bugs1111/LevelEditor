use super::*;

fn source_field_number(raw: &str, kind: &str) -> Value {
    raw.trim()
        .parse::<f64>()
        .ok()
        .filter(|value| value.is_finite())
        .map_or(Value::Null, |value| {
            if kind == "Int" {
                json!(value.trunc() as i32)
            } else {
                json!(value)
            }
        })
}
pub(super) fn source_file_types(raw: &str) -> Value {
    let tokens = raw
        .split(|c: char| !c.is_ascii_alphanumeric() && !matches!(c, '_' | '-' | '.'))
        .filter(|token| !token.is_empty())
        .collect::<Vec<_>>();
    if tokens.is_empty() {
        return Value::Null;
    }
    let mut values = Vec::<String>::new();
    for token in tokens {
        let extension = token
            .strip_prefix('.')
            .unwrap_or(token)
            .to_ascii_lowercase();
        if !extension.is_empty()
            && !extension.contains("..")
            && extension.bytes().any(|c| c.is_ascii_alphanumeric())
        {
            let value = format!(".{extension}");
            if !values.contains(&value) {
                values.push(value);
            }
        }
    }
    json!(values)
}

impl EditorApp {
    pub(super) fn source_field_list(&mut self, ui: &mut egui::Ui, owner: Option<i64>, height: f32) {
        let fields = self
            .project
            .as_ref()
            .map(|p| {
                owner
                    .and_then(|uid| {
                        p.json["defs"]["entities"]
                            .as_array()?
                            .iter()
                            .find(|e| e["uid"] == uid)
                    })
                    .map(|e| &e["fieldDefs"])
                    .unwrap_or(&p.json["defs"]["levelFields"])
            })
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        if !fields.iter().any(|f| f["uid"].as_i64() == self.field_uid) {
            self.field_uid = fields.first().and_then(|f| f["uid"].as_i64());
        }
        ui.spacing_mut().item_spacing.y = FIELD_LIST_GAP;
        let button_row_height = 40.;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.;
            ui.spacing_mut().button_padding.x = 4.;
            for (array, label) in [(false, "+ SINGLE VALUE"), (true, "+ ARRAY")] {
                if orange(
                    ui,
                    label,
                    egui::vec2(
                        if array {
                            70.
                        } else {
                            (ui.available_width() - 74.).max(90.)
                        },
                        button_row_height,
                    ),
                )
                .clicked()
                {
                    ui.ctx().data_mut(|d| {
                        d.insert_temp(
                            egui::Id::new("source-field-type-picker"),
                            (owner, array, None::<i64>, None::<String>),
                        )
                    });
                }
            }
        });
        egui::Frame::new()
            .fill(theme::DARK)
            .stroke(Stroke::new(1_f32, theme::LIGHT))
            .inner_margin(4)
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt(("source-field-list", owner))
                    .max_height((height - button_row_height - FIELD_LIST_GAP - 10.).max(40.))
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = 0.;
                        for (index, field) in fields.iter().enumerate() {
                            let uid = field["uid"].as_i64().unwrap_or(-1);
                            let selected = self.field_uid == Some(uid);
                            let kind = text(field, "__type", "Int")
                                .trim_start_matches("Array<")
                                .trim_end_matches('>');
                            let (badge, color) = match kind {
                                "Int" | "Float" => ("123", "#50b3cb"),
                                "Bool" => ("", "#cd88dd"),
                                "String" | "Multilines" => ("Abc", "#bd5f32"),
                                "Color" => ("#", "#99d367"),
                                "Tile" => ("Tile", "#99d367"),
                                "Point" => ("XY", "#7779c9"),
                                "FilePath" => ("File", "#7779c9"),
                                "EntityRef" => ("Ref", "#7779c9"),
                                _ => ("Enu", "#ff4b4b"),
                            };
                            let item = egui::Frame::new()
                                .fill(if selected {
                                    theme::ACCENT
                                } else {
                                    theme::MEDIUM
                                })
                                .inner_margin(egui::Margin::symmetric(5, 4))
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        let badge_width = 34.;
                                        let menu_size = egui::vec2(18., 30.);
                                        ui.add(
                                            egui::Label::new(RichText::new("≡").color(
                                                if selected {
                                                    Color32::BLACK
                                                } else {
                                                    theme::TEXT
                                                },
                                            ))
                                            .sense(Sense::drag()),
                                        )
                                        .dnd_set_drag_payload((owner, uid));
                                        let name_width = ui.available_width()
                                            - badge_width
                                            - menu_size.x
                                            - 2. * ui.spacing().item_spacing.x;
                                        let (rect, response) = ui.allocate_exact_size(
                                            egui::vec2(name_width.max(25.), 30.),
                                            Sense::click(),
                                        );
                                        let name = text(field, "identifier", "Field");
                                        let ink = if selected {
                                            Color32::BLACK
                                        } else {
                                            Color32::WHITE
                                        };
                                        let inset = 8.;
                                        let galley =
                                            egui::WidgetText::from(RichText::new(name).color(ink))
                                                .into_galley(
                                                    ui,
                                                    Some(egui::TextWrapMode::Truncate),
                                                    (rect.width() - inset).max(0.),
                                                    FontId::proportional(14.),
                                                );
                                        ui.painter_at(rect).galley(
                                            rect.left_center()
                                                + egui::vec2(inset, -galley.size().y / 2.),
                                            galley,
                                            ink,
                                        );
                                        response.widget_info(|| {
                                            egui::WidgetInfo::selected(
                                                egui::WidgetType::SelectableLabel,
                                                true,
                                                selected,
                                                name,
                                            )
                                        });
                                        if response.clicked() {
                                            self.clipboard_focus(
                                                ui.ctx(),
                                                ClipTarget::Field {
                                                    owner,
                                                    uid: Some(uid),
                                                },
                                            );
                                            self.field_uid = Some(uid);
                                        }
                                        egui::Frame::new()
                                            .fill(theme::color(color).gamma_multiply(0.7))
                                            .corner_radius(3.)
                                            .inner_margin(4)
                                            .show(ui, |ui| {
                                                ui.set_width(badge_width - 8.);
                                                if kind == "Bool" {
                                                    let (rect, _) = ui.allocate_exact_size(
                                                        egui::vec2(20., 20.),
                                                        Sense::hover(),
                                                    );
                                                    ui.painter().add(egui::epaint::Shape::line(
                                                        vec![
                                                            rect.min + egui::vec2(3., 10.),
                                                            rect.min + egui::vec2(8., 15.),
                                                            rect.min + egui::vec2(17., 5.),
                                                        ],
                                                        Stroke::new(3_f32, Color32::WHITE),
                                                    ));
                                                } else {
                                                    ui.label(
                                                        RichText::new(badge).color(Color32::WHITE),
                                                    );
                                                }
                                            });
                                        source_definition_menu(
                                            ui,
                                            Some(menu_size),
                                            &response,
                                            |ui| {
                                                self.clipboard_menu(
                                                    ui,
                                                    ClipTarget::Field {
                                                        owner,
                                                        uid: Some(uid),
                                                    },
                                                );
                                            },
                                        );
                                    });
                                });
                            if let Some(payload) =
                                item.response.dnd_hover_payload::<(Option<i64>, i64)>()
                                && payload.0 == owner
                                && payload.1 != uid
                            {
                                ui.painter().hline(
                                    item.response.rect.x_range(),
                                    item.response.rect.top(),
                                    Stroke::new(2_f32, theme::ACCENT),
                                );
                            }
                            if let Some(payload) =
                                item.response.dnd_release_payload::<(Option<i64>, i64)>()
                                && payload.0 == owner
                                && payload.1 != uid
                            {
                                self.edit("Field reordered", |p| {
                                    p.reorder_field_definition(payload.1, index)
                                });
                            }
                            ui.add_space(2.);
                        }
                        self.definition_list_paste_menu(ui, ClipTarget::Field { owner, uid: None });
                    });
            });
    }
}

fn field_display_select<'a>(
    ui: &mut egui::Ui,
    id: &str,
    width: f32,
    value: &mut &'a str,
    options: impl Iterator<Item = (&'a str, &'a str)> + Clone,
) {
    let label = options
        .clone()
        .find(|(key, _)| key == value)
        .or_else(|| options.clone().next())
        .map_or("", |(_, label)| label);
    ui.allocate_ui_with_layout(
        egui::vec2(width, 32.),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.set_max_width(width);
            egui::ComboBox::from_id_salt(id)
                .selected_text(locale::tr(ui, label))
                .width(width)
                .truncate()
                .show_ui(ui, |ui| {
                    for (key, label) in options {
                        ui.selectable_value(value, key, locale::tr(ui, label));
                    }
                });
        },
    );
}

impl EditorApp {
    pub(in crate::app::panels) fn source_field_properties(
        &mut self,
        ui: &mut egui::Ui,
        def: &Value,
    ) {
        let previous_spacing = ui.spacing().item_spacing.y;
        ui.spacing_mut().item_spacing.y = 10.;
        source_controls::form_heading_line(ui, "General settings");
        let uid = def["uid"].as_i64().unwrap();
        let public = text(def, "__type", "Int");
        let kind = public.trim_start_matches("Array<").trim_end_matches('>');
        let array = def["isArray"] == true;
        source_controls::form_row_wrapped(ui, "Type", "Type", FIELD_LABEL_WIDTH, |ui| {
            ui.spacing_mut().item_spacing.x = 0.;
            ui.spacing_mut().button_padding.x = 4.;
            let type_width = (ui.available_width() - 73.).max(40.);
            let (rect, _) = ui.allocate_exact_size(egui::vec2(type_width, 33.), Sense::hover());
            let painter = ui.painter().with_clip_rect(rect.intersect(ui.clip_rect()));
            painter.rect_stroke(
                rect,
                0.,
                Stroke::new(1_f32, theme::LIGHT),
                StrokeKind::Inside,
            );
            painter.text(
                rect.left_center() + egui::vec2(8., 0.),
                Align2::LEFT_CENTER,
                public,
                FontId::monospace(14.),
                theme::TEXT.gamma_multiply(0.6),
            );
            if orange(ui, "CONVERT", egui::vec2(73., 33.)).clicked() {
                ui.ctx().data_mut(|d| {
                    d.insert_temp(
                        egui::Id::new("source-field-type-picker"),
                        (self.field_owner, array, Some(uid), None::<String>),
                    )
                });
            }
        });
        let mut patch = form_with_label_width(
            ui,
            def,
            &[
                P("identifier", "Field identifier", W::Text),
                P("doc", "Documentation", W::Text),
            ],
            FIELD_LABEL_WIDTH,
        );
        if let Some(owner) = self.field_owner {
            let can_export = self.project.as_ref().is_some_and(|project| {
                project.json["defs"]["entities"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .any(|def| def["uid"] == owner && def["exportToToc"] == true)
            });
            source_controls::form_row_wrapped(
                ui,
                "Export to ToC",
                "exportToToc",
                FIELD_LABEL_WIDTH,
                |ui| {
                    let mut checked = def["exportToToc"] == true;
                    if ui
                        .add_enabled_ui(can_export, |ui| source_checkbox(ui, &mut checked, ""))
                        .inner
                        .changed()
                    {
                        patch["exportToToc"] = json!(checked);
                    }
                },
            );
        }
        if matches!(kind, "String" | "Multilines") || kind.contains("Enum.") {
            let searchable = form_with_label_width(
                ui,
                def,
                &[P("searchable", "Searchable", W::Bool)],
                FIELD_LABEL_WIDTH,
            );
            if let Some(value) = searchable.get("searchable") {
                patch["searchable"] = value.clone();
            }
        }
        source_controls::form_heading_line(ui, "Value specifications");
        if !matches!(kind, "Point" | "EntityRef" | "Tile" | "FilePath")
            && let Ok(value) = self.project.as_ref().unwrap().field_default_value(def)
        {
            let label = match kind {
                "Bool" => "Default state",
                "Color" => "Default color",
                _ => "Default value",
            };
            if let Some(value) =
                source_controls::form_row_wrapped(ui, label, label, FIELD_LABEL_WIDTH, |ui| {
                    if matches!(kind, "Int" | "Float") {
                        let current = def["defaultOverride"]["params"][0]
                            .as_number()
                            .map(ToString::to_string);
                        let hint = if value.is_null() {
                            locale::tr(ui, "(null)").to_owned()
                        } else {
                            value.to_string()
                        };
                        let width = ui.available_width();
                        source_controls::edit_text(
                            ui,
                            ("field-numeric-default", uid),
                            current.as_deref().unwrap_or(""),
                            |ui, draft, id| {
                                ui.add_sized(
                                    [width, 33.],
                                    egui::TextEdit::singleline(draft)
                                        .id(id)
                                        .vertical_align(egui::Align::Center)
                                        .hint_text(hint),
                                )
                            },
                        )
                        .map(|raw| source_field_number(&raw, kind))
                    } else {
                        ui.vertical(|ui| {
                            scalar_editor(ui, self.project.as_ref().unwrap(), kind, &value, def)
                        })
                        .inner
                    }
                })
            {
                self.edit("Field default changed", |p| p.set_field_default(uid, value));
            }
        }
        if matches!(kind, "Int" | "Float") {
            source_controls::form_row_wrapped(ui, "Limits", "Limits", FIELD_LABEL_WIDTH, |ui| {
                ui.spacing_mut().item_spacing.x = 4.;
                let width = ((ui.available_width() - 24.) / 2.).max(40.);
                for key in ["min", "max"] {
                    if key == "max" {
                        ui.label(locale::tr(ui, "to"));
                    }
                    let current = def[key].as_f64().map(|value| value.to_string());
                    if let Some(raw) = ui_fields::buffered_text_sized(
                        ui,
                        key,
                        current.as_deref().unwrap_or(""),
                        egui::vec2(width, 33.),
                    ) {
                        patch[key] = source_field_number(&raw, kind);
                    }
                }
            });
        }
        let mut properties = Vec::new();
        if !matches!(kind, "Bool" | "Tile") {
            properties.push(P(
                "canBeNull",
                if array {
                    "Can contain nulls"
                } else if matches!(kind, "FilePath" | "Point" | "EntityRef") {
                    "Is optional"
                } else {
                    "Can be null"
                },
                W::Bool,
            ));
        }
        if kind == "FilePath" {
            properties.push(P("acceptFileTypes", "File extensions", W::Text));
        }
        if kind == "String" {
            properties.push(P("regex", "Regex check", W::Text));
        }
        if kind == "Multilines" {
            let previous = def["textLanguageMode"].as_str().map(str::to_owned);
            let mut language = previous.clone();
            source_controls::form_row_wrapped(
                ui,
                "Language",
                "textLanguageMode",
                FIELD_LABEL_WIDTH,
                |ui| {
                    egui::ComboBox::from_id_salt(("field-language", uid))
                        .selected_text(locale::tr(ui, language.as_deref().unwrap_or("Plain text")))
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut language, None, locale::tr(ui, "Plain text"));
                            for (mode, label) in [
                                ("LangJson", "JSON"),
                                ("LangXml", "XML/HTML"),
                                ("LangMarkdown", "Markdown"),
                                ("LangRuby", "Ruby"),
                                ("LangPython", "Python"),
                                ("LangHaxe", "Haxe"),
                                ("LangJS", "Javascript"),
                                ("LangLua", "Lua"),
                                ("LangC", "C/C++/C#"),
                                ("LangLog", "Log file"),
                            ] {
                                ui.selectable_value(
                                    &mut language,
                                    Some(mode.to_owned()),
                                    locale::tr(ui, label),
                                );
                            }
                        });
                },
            );
            if language != previous {
                patch["textLanguageMode"] = language.map(Value::String).unwrap_or(Value::Null);
            }
        }
        let mut specs = def.clone();
        if kind == "FilePath" {
            specs["acceptFileTypes"] = json!(
                def["acceptFileTypes"]
                    .as_array()
                    .map(|values| values
                        .iter()
                        .filter_map(Value::as_str)
                        .collect::<Vec<_>>()
                        .join("  "))
                    .unwrap_or_default()
            );
        }
        if let Some(obj) =
            form_with_label_width(ui, &specs, &properties, FIELD_LABEL_WIDTH).as_object()
        {
            patch.as_object_mut().unwrap().extend(obj.clone());
        }
        if kind == "FilePath"
            && let Some(raw) = patch["acceptFileTypes"].as_str()
        {
            patch["acceptFileTypes"] = source_file_types(raw);
        }
        let mut tile_default_edit = None;
        if kind == "Tile" {
            let project = self.project.as_ref().unwrap();
            let atlas = def["tilesetUid"].as_i64();
            let bound = atlas.is_some_and(|atlas| project.tileset(atlas).is_some());
            let tile = if bound {
                project.field_default_value(def).unwrap_or(Value::Null)
            } else {
                Value::Null
            };
            let optional_label = if array {
                "Can contain nulls"
            } else {
                "Is optional"
            };
            source_controls::form_row_wrapped(
                ui,
                optional_label,
                "canBeNull",
                FIELD_LABEL_WIDTH,
                |ui| {
                    let mut optional = def["canBeNull"] == true;
                    if ui
                        .add_enabled_ui(tile.is_null(), |ui| source_checkbox(ui, &mut optional, ""))
                        .inner
                        .changed()
                    {
                        patch["canBeNull"] = json!(optional);
                    }
                },
            );
            source_controls::form_row_wrapped(
                ui,
                "Tileset",
                "tilesetUid",
                FIELD_LABEL_WIDTH,
                |ui| {
                    let placeholder = locale::tr(ui, "-- Pick a tileset --");
                    let mut candidates = Vec::new();
                    if atlas.is_none() {
                        candidates.push((None, placeholder));
                    }
                    candidates.extend(
                        self.project.as_ref().unwrap().json["defs"]["tilesets"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter_map(|tileset| {
                                Some((
                                    Some(tileset["uid"].as_i64()?),
                                    tileset["identifier"].as_str()?,
                                ))
                            }),
                    );
                    let options: Vec<_> =
                        candidates.iter().map(|(_, label)| (*label, None)).collect();
                    let current = candidates.iter().position(|(id, _)| *id == atlas);
                    let title = current.map_or("", |index| options[index].0);
                    let appearance = if atlas.is_none() {
                        source_controls::FormSelectAppearance::Required
                    } else {
                        source_controls::FormSelectAppearance::Normal
                    };
                    let width = if bound {
                        ui.available_width() * 0.5
                    } else {
                        ui.available_width()
                    };
                    let selected = ui
                        .allocate_ui_with_layout(
                            egui::vec2(width, 29.),
                            egui::Layout::left_to_right(egui::Align::Center),
                            |ui| {
                                source_controls::form_select(
                                    ui,
                                    ("field-tile-atlas", uid),
                                    29.,
                                    &options,
                                    current,
                                    title,
                                    appearance,
                                )
                            },
                        )
                        .inner;
                    if let Some(index) = selected.filter(|i| Some(*i) != current) {
                        patch["tilesetUid"] = json!(candidates[index].0);
                    }
                    if bound {
                        ui.weak(locale::tr(ui, "Default:"));
                        tile_default_edit = ui
                            .push_id(("field-default-tile", uid), |ui| {
                                self.value_tile_thumbnail(ui, atlas, &tile, true, false)
                            })
                            .inner;
                        if !tile.is_null() {
                            let image = egui::Image::new(egui::include_image!(
                                "../../assets/icons/clear.svg"
                            ))
                            .fit_to_exact_size(Vec2::splat(18.));
                            if source_controls::add_sized_button(
                                ui,
                                egui::vec2(42., 35.),
                                egui::Button::image(image).fill(theme::LIGHT),
                            )
                            .on_hover_text(locale::tr(ui, "Clear default value"))
                            .clicked()
                            {
                                tile_default_edit = Some(Value::Null);
                            }
                        }
                    }
                },
            );
        }
        if kind == "EntityRef" {
            let policy = text(def, "allowedRefs", "OnlySame");
            source_controls::form_row_wrapped(
                ui,
                "Reference options",
                "Reference options",
                FIELD_LABEL_WIDTH,
                |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing.x = 16.;
                        ui.style_mut()
                            .text_styles
                            .insert(egui::TextStyle::Button, FontId::proportional(13.333));
                        for (key, label) in [
                            ("symmetricalRef", "Symmetrical"),
                            ("allowOutOfLevelRef", "In any level"),
                            ("autoChainRef", "Auto-chain"),
                        ] {
                            if key == "autoChainRef" {
                                ui.end_row();
                            }
                            let mut checked = def[key] == true;
                            if ui
                                .add_enabled_ui(
                                    key != "symmetricalRef" || policy == "OnlySame",
                                    |ui| source_checkbox(ui, &mut checked, label),
                                )
                                .inner
                                .changed()
                            {
                                patch[key] = json!(checked);
                            }
                        }
                    });
                },
            );
            source_controls::form_row_wrapped(
                ui,
                "Allowed references",
                "allowedRefs",
                FIELD_LABEL_WIDTH,
                |ui| {
                    ui.vertical(|ui| {
                        use source_controls::{searchable_select_button, searchable_select_popup};
                        let project = self.project.as_ref().unwrap();
                        let entities = project.json["defs"]["entities"].as_array().unwrap();
                        let owner_name = entities
                            .iter()
                            .find(|entity| entity["uid"].as_i64() == self.field_owner)
                            .and_then(|entity| entity["identifier"].as_str())
                            .unwrap_or("");
                        let same =
                            locale::format(ui, "Only another '{name}'s", &[("name", owner_name)]);
                        let policies = ["Any", "OnlySame", "OnlyTags", "OnlySpecificEntity"];
                        let options = [
                            locale::tr(ui, "Any entity"),
                            same.as_str(),
                            locale::tr(ui, "Any entity with one of the specified tags"),
                            locale::tr(ui, "Only a specific Entity"),
                        ]
                        .map(|label| (label, Color32::TRANSPARENT));
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 4.;
                            let current = policies
                                .iter()
                                .position(|value| *value == policy)
                                .unwrap_or(1);
                            let width = ui.available_width();
                            let width = if policy == "OnlySpecificEntity" {
                                (width - ui.spacing().item_spacing.x) / 2.
                            } else {
                                width
                            };
                            let button = searchable_select_button(ui, width, options[current].0);
                            if let Some((index, _)) = searchable_select_popup(
                                &button,
                                ui.make_persistent_id(("field-reference-policy", uid)),
                                current,
                                &options,
                            )
                            .filter(|(index, _)| *index != current)
                            {
                                let next = policies[index];
                                patch["allowedRefs"] = json!(next);
                                if matches!(next, "Any" | "OnlyTags") {
                                    patch["symmetricalRef"] = json!(false);
                                }
                                if next == "OnlySpecificEntity" {
                                    patch["allowedRefsEntityUid"] = json!(self.field_owner);
                                }
                            }
                            if policy == "OnlySpecificEntity" {
                                let options: Vec<_> = entities
                                    .iter()
                                    .map(|entity| {
                                        (text(entity, "identifier", "Entity"), Color32::TRANSPARENT)
                                    })
                                    .collect();
                                let current = entities.iter().position(|entity| {
                                    entity["uid"] == def["allowedRefsEntityUid"]
                                });
                                let title = current.map_or_else(
                                    || locale::tr(ui, "Choose entity"),
                                    |index| options[index].0,
                                );
                                let button =
                                    searchable_select_button(ui, ui.available_width(), title);
                                if let Some((index, _)) = searchable_select_popup(
                                    &button,
                                    ui.make_persistent_id(("field-reference-type", uid)),
                                    current.unwrap_or(0),
                                    &options,
                                )
                                .filter(|(index, _)| Some(*index) != current)
                                {
                                    patch["allowedRefsEntityUid"] = entities[index]["uid"].clone();
                                }
                            }
                        });
                        if policy == "OnlyTags"
                            && let Some(tag_editor::Change::Set(tags)) = tag_editor::show(
                                ui,
                                project,
                                tag_editor::Field::ReferenceTargets,
                                uid,
                                &def["allowedRefTags"],
                            )
                        {
                            patch["allowedRefTags"] = tags;
                        }
                    });
                },
            );
        }
        if array {
            use source_controls::{NumberStyle, number_input_sized};
            source_controls::form_row_wrapped(
                ui,
                "Array length",
                "arrayMinLength",
                FIELD_LABEL_WIDTH,
                |ui| {
                    ui.spacing_mut().item_spacing.x = 4.;
                    let width = ((ui.available_width() - 24.) / 2.).max(40.);
                    for (key, opposite, placeholder, minimum) in [
                        ("arrayMinLength", "arrayMaxLength", "min length", true),
                        ("arrayMaxLength", "arrayMinLength", "max length", false),
                    ] {
                        if !minimum {
                            ui.label(locale::tr(ui, "to"));
                        }
                        let current = def[key].as_i64().map(|n| n.to_string());
                        if let Some(raw) = number_input_sized(
                            ui,
                            key,
                            current.as_deref().unwrap_or(""),
                            NumberStyle::Plain {
                                placeholder: Some(placeholder),
                            },
                            width,
                        ) {
                            let length = raw.trim().parse::<i64>().unwrap_or(0).clamp(0, 99_999);
                            patch[key] = if length == 0 {
                                Value::Null
                            } else {
                                json!(length)
                            };
                            if length > 0
                                && def[opposite].as_i64().is_some_and(|bound| {
                                    if minimum {
                                        bound < length
                                    } else {
                                        bound > length
                                    }
                                })
                            {
                                patch[opposite] = json!(length);
                            }
                        }
                    }
                },
            );
        }
        source_controls::form_heading_line(ui, "Display");
        ui.push_id(uid, |ui| {
            self.source_field_display(ui, def, kind, array, &mut patch)
        });
        ui.spacing_mut().item_spacing.y = previous_spacing;
        if changed(&patch) || tile_default_edit.is_some() {
            self.edit("Field definition changed", |p| {
                if changed(&patch) {
                    p.update_field_definition(uid, &patch)?;
                }
                if let Some(tile) = tile_default_edit {
                    p.set_field_default(uid, tile)?;
                }
                Ok(())
            });
        }
    }
    fn source_field_display(
        &self,
        ui: &mut egui::Ui,
        def: &Value,
        kind: &str,
        array: bool,
        patch: &mut Value,
    ) {
        let entity = self.field_owner.is_some();
        let point = kind == "Point";
        let reference = kind == "EntityRef";
        let number = matches!(kind, "Int" | "Float");
        let enumeration = kind.contains("Enum.");
        let tile = kind == "Tile" || enumeration;
        let path = kind == "FilePath";
        let identifier = text(def, "identifier", "Field");
        let name_and_value = if array {
            locale::format(
                ui,
                r#"Show "{identifier}=[...values...]""#,
                &[("identifier", identifier)],
            )
        } else {
            locale::format(
                ui,
                r#"Show "{identifier}=...""#,
                &[("identifier", identifier)],
            )
        };
        let value_only = if array {
            "Show values only"
        } else {
            "Show value only"
        };
        let isolated_point = if array {
            "Show isolated points"
        } else {
            "Show isolated point"
        };
        let connected_point = if array {
            "Show star of points"
        } else {
            "Show connected point"
        };
        let entity_tile = if path {
            "Replace entity tile (image file only)"
        } else {
            "Replace entity tile"
        };
        let options = [
            ("Hidden", "Do not show", true),
            ("ValueOnly", value_only, !point),
            ("NameAndValue", &name_and_value, true),
            (
                "EntityTile",
                entity_tile,
                entity && (tile || path && !array),
            ),
            (
                "LevelTile",
                "Replace level render in world view",
                !entity && tile,
            ),
            ("Points", isolated_point, entity && point),
            ("PointStar", connected_point, entity && point),
            ("PointPath", "Show path of points", entity && point && array),
            (
                "PointPathLoop",
                "Show path of points (looping)",
                entity && point && array,
            ),
            (
                "RadiusPx",
                "As a radius (pixels)",
                entity && number && !array,
            ),
            (
                "RadiusGrid",
                "As a radius (grid-based)",
                entity && number && !array,
            ),
            ("ArrayCountWithLabel", "Show array length with label", array),
            ("ArrayCountNoLabel", "Show array length only", array),
            (
                "RefLinkBetweenPivots",
                "Reference link (using pivot coord)",
                reference,
            ),
            (
                "RefLinkBetweenCenters",
                "Reference link (using center coord)",
                reference,
            ),
        ];
        let current = text(def, "editorDisplayMode", "Hidden");
        let mut mode = current;
        let text_mode = matches!(
            current,
            "ValueOnly" | "NameAndValue" | "ArrayCountWithLabel" | "ArrayCountNoLabel"
        );
        let link = matches!(
            current,
            "PointStar"
                | "PointPath"
                | "PointPathLoop"
                | "RefLinkBetweenPivots"
                | "RefLinkBetweenCenters"
        );
        let color = !matches!(current, "Hidden" | "EntityTile" | "LevelTile");
        source_controls::form_row_wrapped(
            ui,
            "Display in editor",
            "editorDisplayMode",
            FIELD_LABEL_WIDTH,
            |ui| {
                ui.spacing_mut().item_spacing.x = 4.;
                ui.horizontal_wrapped(|ui| {
                    let position_width = if entity && text_mode { 114. } else { 0. };
                    let link_width = if link { 137. } else { 0. };
                    let world_width = if entity { 0. } else { 185. };
                    let scale_width = if text_mode { 62. } else { 0. };
                    let color_width = if color { 70. } else { 0. };
                    let accessory_width =
                        position_width + link_width + world_width + scale_width + color_width;
                    let mode_width = (ui.available_width() - accessory_width).max(64.);
                    field_display_select(
                        ui,
                        "field-display-mode",
                        mode_width,
                        &mut mode,
                        options
                            .into_iter()
                            .filter_map(|(key, label, allowed)| allowed.then_some((key, label))),
                    );
                    if entity && text_mode {
                        let original = text(def, "editorDisplayPos", "Above");
                        let mut position = original;
                        field_display_select(
                            ui,
                            "field-display-position",
                            position_width - 4.,
                            &mut position,
                            ["Above", "Center", "Beneath"]
                                .into_iter()
                                .map(|option| (option, option)),
                        );
                        if position != original {
                            patch["editorDisplayPos"] = json!(position);
                        }
                    }
                    if link {
                        let original = text(def, "editorLinkStyle", "ZigZag");
                        let mut style = original;
                        field_display_select(
                            ui,
                            "field-link-style",
                            link_width - 4.,
                            &mut style,
                            [
                                ("ZigZag", "Zig-zag"),
                                ("StraightArrow", "Straight arrow"),
                                ("CurvedArrow", "Curved arrow"),
                                ("ArrowsLine", "Line of arrows"),
                                ("DashedLine", "Dashed line"),
                            ]
                            .into_iter(),
                        );
                        if style != original {
                            patch["editorLinkStyle"] = json!(style);
                        }
                    }
                    if !entity {
                        let mut world_view = def["editorShowInWorld"].as_bool().unwrap_or(true);
                        let checkbox = ui.allocate_ui_with_layout(
                            egui::vec2(world_width - 4., 32.),
                            egui::Layout::left_to_right(egui::Align::Center),
                            |ui| source_checkbox(ui, &mut world_view, "Show in world view"),
                        );
                        if checkbox.inner.changed() {
                            patch["editorShowInWorld"] = json!(world_view);
                        }
                    }
                    if text_mode {
                        let before =
                            (def["editorDisplayScale"].as_f64().unwrap_or(1.) * 100.).round();
                        let mut percent = before;
                        ui_commit::prepare_editor(ui.ctx(), ui.next_auto_id());
                        let response = ui.add_sized(
                            [scale_width - 4., 32.],
                            egui::DragValue::new(&mut percent)
                                .speed(1.)
                                .range(10. ..=3_276_700.)
                                .clamp_existing_to_range(false)
                                .max_decimals(0)
                                .update_while_editing(false)
                                .custom_parser(|raw| {
                                    Some(
                                        raw.trim()
                                            .trim_end_matches('%')
                                            .trim_end()
                                            .parse::<f64>()
                                            .ok()
                                            .filter(|value| value.is_finite())
                                            .unwrap_or(100.)
                                            .round(),
                                    )
                                }),
                        );
                        ui_commit::track_editor(ui.ctx(), &response);
                        response.on_hover_text(locale::tr(ui, "Display scale (%)"));
                        if percent != before {
                            patch["editorDisplayScale"] = json!(percent.round() / 100.);
                        }
                    }
                    if color {
                        let project = self.project.as_ref().unwrap();
                        let inherited = if let Some(owner) = self.field_owner {
                            let entity = &project.json["defs"]["entities"];
                            let entity = entity
                                .as_array()
                                .into_iter()
                                .flatten()
                                .find(|def| def["uid"] == owner);
                            entity
                                .and_then(|def| def["color"].as_str())
                                .map_or(theme::TEXT, theme::color)
                        } else {
                            let background = self
                                .selected
                                .and_then(|key| project.level(key))
                                .map_or_else(
                                    || {
                                        theme::color(text(
                                            &project.json,
                                            "defaultLevelBgColor",
                                            "#696A79",
                                        ))
                                    },
                                    |level| canvas::level_smart_color(project, level),
                                );
                            background.lerp_to_gamma(Color32::WHITE, 0.45)
                        };
                        let mut value = def["editorDisplayColor"]
                            .as_str()
                            .map(theme::color)
                            .map(|c| [c.r(), c.g(), c.b()]);
                        if source_colors::inherited_color_editor(ui, &mut value, inherited)
                            .changed()
                        {
                            patch["editorDisplayColor"] = value.map_or(Value::Null, |[r, g, b]| {
                                json!(format!("#{r:02X}{g:02X}{b:02X}"))
                            });
                        }
                    }
                });
            },
        );
        if mode != current {
            patch["editorDisplayMode"] = json!(mode);
        }
        if number {
            source_controls::form_row_wrapped(
                ui,
                "Prefix/suffix",
                "editorTextPrefix",
                FIELD_LABEL_WIDTH,
                |ui| {
                    ui.spacing_mut().item_spacing.x = 4.;
                    let width = ((ui.available_width() - 39.) / 2.).max(30.);
                    let affix_size = egui::vec2(width, 33.);
                    for (key, prefix) in [("editorTextPrefix", true), ("editorTextSuffix", false)] {
                        if !prefix {
                            ui.label("1234");
                        }
                        if let Some(value) =
                            ui_fields::buffered_text_sized(ui, key, text(def, key, ""), affix_size)
                        {
                            let value = if prefix {
                                value.trim_start()
                            } else {
                                value.trim_end()
                            };
                            patch[key] = if value.is_empty() {
                                Value::Null
                            } else {
                                json!(value)
                            };
                        }
                    }
                },
            );
        }
        let color_label = if entity {
            "Override entity color"
        } else {
            "Override level color"
        };
        for (key, label, visible, enabled) in [
            ("editorAlwaysShow", "Always show", true, mode != "Hidden"),
            (
                "editorCutLongValues",
                "Only show an excerpt in editor",
                matches!(kind, "String" | "Multilines"),
                true,
            ),
            (
                "useForSmartColor",
                color_label,
                kind == "Color" || enumeration,
                true,
            ),
        ] {
            if visible {
                source_controls::form_row_wrapped(ui, label, key, FIELD_LABEL_WIDTH, |ui| {
                    let mut checked = def[key].as_bool().unwrap_or(false);
                    if ui
                        .add_enabled_ui(enabled, |ui| source_checkbox(ui, &mut checked, ""))
                        .inner
                        .changed()
                    {
                        patch[key] = json!(checked);
                    }
                });
            }
        }
    }

    pub(super) fn source_field_type_picker(&mut self, ctx: &egui::Context) {
        let id = egui::Id::new("source-field-type-picker");
        let Some((owner, array, convert, mut chosen)) =
            ctx.data_mut(|d| d.get_temp::<(Option<i64>, bool, Option<i64>, Option<String>)>(id))
        else {
            return;
        };
        let mut open = true;
        let mut dismiss = false;
        let mut apply = None;
        let title = if convert.is_some() {
            "Convert field type"
        } else if array {
            "Create array field"
        } else {
            "Create single value field"
        };
        egui::Window::new(locale::tr_ctx(ctx, title))
            .id(id)
            .open(&mut open)
            .resizable(false)
            .show(ctx, |ui| {
                ui.horizontal_wrapped(|ui| {
                    for kind in [
                        "Int", "Float", "Bool", "String", "Multilines", "Color", "FilePath",
                        "Tile", "EntityRef", "Point",
                    ] {
                        if owner.is_none() && matches!(kind, "EntityRef" | "Point") {
                            continue;
                        }
                        let button = egui::Button::new(locale::tr(ui, kind));
                        if source_controls::add_sized_button(ui, [95., 44.], button).clicked() {
                            chosen = Some(kind.into());
                            if convert.is_none() {
                                apply = chosen.clone();
                            }
                        }
                    }
                });
                ui.separator();
                ui.label(locale::tr(ui, "Enums"));
                let project = self.project.as_ref().unwrap();
                for (group, prefix) in [("enums", "LocalEnum"), ("externalEnums", "ExternEnum")] {
                    for def in project.json["defs"][group].as_array().into_iter().flatten() {
                        let name = text(def, "identifier", "Enum");
                        if source_controls::button(ui, RichText::new(name)).clicked() {
                            chosen = Some(format!("{prefix}.{name}"));
                            if convert.is_none() {
                                apply = chosen.clone();
                            }
                        }
                    }
                }
                if project.json["defs"]["enums"].as_array().is_none_or(Vec::is_empty)
                    && project.json["defs"]["externalEnums"].as_array().is_none_or(Vec::is_empty)
                {
                    ui.label(locale::tr(ui, "Create an enum in the Enum panel first."));
                    if source_controls::button(ui, "Open enum panel").clicked() {
                        self.show_panel(Some(Panel::Enums));
                        dismiss = true;
                    }
                }
                if convert.is_some() && let Some(kind) = &chosen {
                    ui.separator();
                    ui.colored_label(
                        theme::ACCENT,
                        locale::tr(ui, "Converting changes existing values and defaults. Values incompatible with the chosen type will be cleared."),
                    );
                    let label = if kind.contains('.') { kind } else { locale::tr(ui, kind) };
                    ui.label(locale::format(ui, "Convert to {kind}", &[("kind", label)]));
                    if source_controls::button(ui, "Convert all values").clicked() {
                        apply = chosen.clone();
                    }
                }
            });
        if dismiss {
            open = false;
        }
        if let Some(kind) = apply {
            if let Some(uid) = convert {
                if self
                    .edit_recoverable("Field type converted", |p| {
                        p.convert_field_type(uid, &kind, array, true)
                    })
                    .is_some()
                {
                    open = false;
                }
            } else {
                let fields = self.project.as_ref().unwrap();
                let fields = owner
                    .and_then(|uid| {
                        fields.json["defs"]["entities"]
                            .as_array()
                            .and_then(|a| a.iter().find(|d| d["uid"] == uid))
                    })
                    .map(|d| &d["fieldDefs"])
                    .unwrap_or(&fields.json["defs"]["levelFields"]);
                let base = kind.split('.').next_back().unwrap_or(&kind);
                let name = unique_name(
                    base,
                    fields
                        .as_array()
                        .into_iter()
                        .flatten()
                        .map(|d| text(d, "identifier", "")),
                );
                if let Some(uid) = self.edit("Field created", |p| {
                    p.add_field_definition(owner, &name, &kind, array)
                }) {
                    self.field_uid = Some(uid);
                    open = false;
                }
            }
        }
        if open {
            ctx.data_mut(|d| d.insert_temp(id, (owner, array, convert, chosen)));
        } else {
            ctx.data_mut(|d| d.remove::<(Option<i64>, bool, Option<i64>, Option<String>)>(id));
        }
    }
}
