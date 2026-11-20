use super::*;

#[derive(Clone)]
pub(super) enum Destination {
    World(Option<usize>),
    Level(LevelKey),
    LevelField(LevelKey),
    Layer(LevelKey, usize),
    Entity(String),
    Definition(Panel, i64),
}

#[derive(Clone, Copy)]
enum Category {
    Definition,
    World,
    Level,
    LevelField,
    Layer,
    Entity,
    EntityField,
}

impl Category {
    fn keywords(self) -> &'static str {
        match self {
            Self::Definition => "definition",
            Self::World => "world",
            Self::Level => "level",
            Self::LevelField => "level field",
            Self::Layer => "layer",
            Self::Entity => "entity",
            Self::EntityField => "entity field",
        }
    }

    fn appearance(self) -> (&'static str, Color32, egui::ImageSource<'static>) {
        match self {
            Self::Definition => (
                "DEF",
                theme::ACCENT,
                egui::include_image!("../../assets/icons/projectSettings.png"),
            ),
            Self::World => (
                "WORLD",
                Color32::from_rgb(0x94, 0x48, 0x3b),
                egui::include_image!("../../assets/icons/world.svg"),
            ),
            Self::Level => (
                "LEVEL",
                Color32::from_rgb(0x65, 0xd9, 0xff),
                egui::include_image!("../../assets/icons/level.svg"),
            ),
            Self::Layer => (
                "LAYER",
                Color32::from_rgb(0x82, 0x4c, 0xff),
                egui::include_image!("../../assets/icons/layer.png"),
            ),
            Self::Entity => (
                "ENTITY",
                Color32::from_rgb(0xff, 0x95, 0x54),
                egui::include_image!("../../assets/icons/entity.png"),
            ),
            Self::LevelField => (
                "LFIELD",
                Color32::from_rgb(0x36, 0x97, 0xb7),
                egui::include_image!("../../assets/icons/list.svg"),
            ),
            Self::EntityField => (
                "EFIELD",
                Color32::from_rgb(0xff, 0x7d, 0x2d),
                egui::include_image!("../../assets/icons/list.svg"),
            ),
        }
    }
}

#[derive(Clone)]
pub(super) struct Entry {
    category: Category,
    title: String,
    context: String,
    keywords: String,
    destination: Destination,
}

fn keywords(raw: &str) -> String {
    raw.to_lowercase()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '_' {
                c
            } else {
                ' '
            }
        })
        .collect()
}

fn matching<'a>(entries: &'a [Entry], query: &str) -> impl Iterator<Item = &'a Entry> {
    let query = keywords(query);
    entries.iter().filter(move |entry| {
        !query.is_empty()
            && query
                .split_whitespace()
                .all(|word| entry.keywords.contains(word))
    })
}

fn values(value: &Value) -> &[Value] {
    value.as_array().map(Vec::as_slice).unwrap_or_default()
}

fn field_display(project: &Project, kind: &str, value: &Value, def: &Value) -> String {
    match kind {
        "String" | "Text" | "Multilines" | "FilePath" => {
            format!("\"{}\"", value.as_str().unwrap_or(""))
        }
        "Int" | "Float" => {
            let number = value
                .as_i64()
                .map(|n| n.to_string())
                .or_else(|| value.as_f64().map(|n| n.to_string()))
                .unwrap_or_else(|| value.to_string());
            format!(
                "{}{number}{}",
                text(def, "editorTextPrefix", ""),
                text(def, "editorTextSuffix", "")
            )
        }
        "Point" => format!("{},{}", value["cx"], value["cy"]),
        "Tile" => format!(
            "{},{},{},{}",
            value["x"], value["y"], value["w"], value["h"]
        ),
        "EntityRef" => {
            let found = value["entityIid"].as_str().and_then(|iid| {
                project.level_keys().into_iter().find_map(|key| {
                    let level = project.level(key)?;
                    values(&level["layerInstances"])
                        .iter()
                        .flat_map(|layer| values(&layer["entityInstances"]))
                        .find(|entity| entity["iid"] == iid)
                        .map(|entity| {
                            format!(
                                "@({} in {})",
                                text(entity, "__identifier", "Entity"),
                                text(level, "identifier", "Level")
                            )
                        })
                })
            });
            found.unwrap_or_else(|| "@(Lost reference!)".into())
        }
        _ => value
            .as_str()
            .map(str::to_owned)
            .unwrap_or_else(|| value.to_string()),
    }
}

fn searchable_values<'a>(
    project: &'a Project,
    fields: &'a Value,
    definitions: &'a Value,
) -> impl Iterator<Item = String> + 'a {
    values(fields)
        .iter()
        .filter_map(move |field| {
            let def = values(definitions)
                .iter()
                .find(|def| def["uid"] == field["defUid"] && def["searchable"] == true)?;
            let kind = text(field, "__type", "");
            let kind = kind
                .strip_prefix("Array<")
                .and_then(|s| s.strip_suffix('>'))
                .unwrap_or(kind);
            let value = &field["__value"];
            let items = value
                .as_array()
                .map(Vec::as_slice)
                .unwrap_or_else(|| std::slice::from_ref(value));
            Some(
                items
                    .iter()
                    .filter(|item| !item.is_null())
                    .map(move |item| field_display(project, kind, item, def)),
            )
        })
        .flatten()
}

fn search_index(project: &Project) -> Vec<Entry> {
    let mut entries = Vec::new();
    let mut add =
        |category: Category, title: String, context: &str, terms: &[&str], destination| {
            let terms = format!("{} {}", terms.join(" "), category.keywords());
            entries.push(Entry {
                category,
                title,
                context: context.into(),
                keywords: keywords(&terms),
                destination,
            });
        };

    let defs = &project.json["defs"];
    for (kind, panel) in [
        ("layers", Panel::Layers),
        ("entities", Panel::Entities),
        ("enums", Panel::Enums),
        ("externalEnums", Panel::Enums),
        ("tilesets", Panel::Tilesets),
    ] {
        for def in values(&defs[kind]) {
            if let (Some(name), Some(uid)) = (def["identifier"].as_str(), def["uid"].as_i64()) {
                add(
                    Category::Definition,
                    name.into(),
                    "",
                    &[name],
                    Destination::Definition(panel, uid),
                );
            }
        }
    }

    let explicit_worlds = values(&project.json["worlds"]);
    let worlds = if explicit_worlds.is_empty() {
        vec![(None, "World", text(&project.json, "dummyWorldIid", ""))]
    } else {
        explicit_worlds
            .iter()
            .enumerate()
            .map(|(index, world)| {
                (
                    Some(index),
                    text(world, "identifier", "World"),
                    text(world, "iid", ""),
                )
            })
            .collect()
    };
    let keys = project.level_keys();
    for (world, name, world_iid) in worlds {
        add(
            Category::World,
            name.into(),
            "",
            &[name, world_iid],
            Destination::World(world),
        );
        for &key in keys.iter().filter(|key| key.world == world) {
            let Some(level) = project.level(key) else {
                continue;
            };
            let level_name = text(level, "identifier", "Level");
            add(
                Category::Level,
                level_name.into(),
                name,
                &[level_name, name, text(level, "iid", "")],
                Destination::Level(key),
            );
            for display in
                searchable_values(project, &level["fieldInstances"], &defs["levelFields"])
            {
                add(
                    Category::LevelField,
                    format!("{level_name}.{display}"),
                    level_name,
                    &[&display],
                    Destination::LevelField(key),
                );
            }
            let layers = values(&level["layerInstances"]);
            for (layer_index, layer) in layers.iter().enumerate() {
                let layer_name = text(layer, "__identifier", "Layer");
                add(
                    Category::Layer,
                    layer_name.into(),
                    level_name,
                    &[text(layer, "iid", "")],
                    Destination::Layer(key, layer_index),
                );
            }
            for entity in layers
                .iter()
                .flat_map(|layer| values(&layer["entityInstances"]))
            {
                let Some(iid) = entity["iid"].as_str() else {
                    continue;
                };
                let name = text(entity, "__identifier", "Entity");
                let def = values(&defs["entities"])
                    .iter()
                    .find(|def| def["uid"] == entity["defUid"]);
                let display_values = def
                    .map(|def| {
                        searchable_values(project, &entity["fieldInstances"], &def["fieldDefs"])
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                let mut title = name.to_owned();
                for display in &display_values {
                    title.push('.');
                    title.push_str(display);
                }
                add(
                    Category::Entity,
                    title,
                    level_name,
                    &[name, iid],
                    Destination::Entity(iid.into()),
                );
                for display in display_values {
                    add(
                        Category::EntityField,
                        format!("{name}.{display}"),
                        name,
                        &[&display],
                        Destination::Entity(iid.into()),
                    );
                }
            }
        }
    }
    entries
}

const PREVIEW_ROWS: usize = 21;
const ROW_HEIGHT: f32 = 23.333;

fn result_row(ui: &mut egui::Ui, entry: &Entry, selected: bool) -> egui::Response {
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), ROW_HEIGHT), Sense::click());
    if !ui.is_rect_visible(rect) {
        return response;
    }
    let (kind, color, icon) = entry.category.appearance();
    let fill = if selected {
        color
    } else {
        Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 38)
    };
    ui.painter().rect_filled(rect, 0., fill);
    let foreground = if selected { Color32::BLACK } else { color };
    let icon_rect = Rect::from_center_size(
        egui::pos2(rect.left() + 25., rect.center().y),
        Vec2::splat(16.),
    );
    egui::Image::new(icon).paint_at(ui, icon_rect);
    ui.painter().text(
        rect.left_center() + egui::vec2(42., 0.),
        Align2::LEFT_CENTER,
        locale::tr(ui, kind),
        FontId::new(9.333, egui::FontFamily::Name("body-bold".into())),
        Color32::from_white_alpha(204),
    );

    let context = match entry.destination {
        Destination::Definition(..) => locale::tr(ui, "Definition"),
        Destination::Level(LevelKey { world: None, .. }) => locale::tr(ui, "World"),
        _ => entry.context.as_str(),
    };
    let context = ui.painter().layout_no_wrap(
        context.into(),
        FontId::proportional(12.),
        foreground.gamma_multiply(0.5),
    );
    let right = rect.right() - 16.;
    let context_left = right - context.size().x;
    ui.painter().galley(
        egui::pos2(context_left, rect.center().y - context.size().y / 2.),
        context,
        foreground,
    );
    let left = rect.left() + 90.;
    let title_clip = Rect::from_min_max(
        egui::pos2(left, rect.top()),
        egui::pos2((context_left - 8.).max(left), rect.bottom()),
    );
    let title: std::borrow::Cow<'_, str> = match entry.destination {
        Destination::World(None) => locale::tr(ui, "World").into(),
        Destination::Level(_) => {
            locale::format(ui, "Level {name}", &[("name", &entry.title)]).into()
        }
        Destination::Layer(..) => {
            locale::format(ui, "Layer {name}", &[("name", &entry.title)]).into()
        }
        _ => std::borrow::Cow::Borrowed(entry.title.as_str()),
    };
    ui.painter().with_clip_rect(title_clip).text(
        egui::pos2(left, rect.center().y),
        Align2::LEFT_CENTER,
        title.as_ref(),
        FontId::proportional(13.333),
        foreground,
    );
    response
}

impl EditorApp {
    fn focus_search_level(&mut self, key: LevelKey) {
        let Some(project) = &self.project else {
            return;
        };
        let placements = canvas::placements(project, key);
        let origin = |key| {
            placements
                .iter()
                .find(|(placed, _)| *placed == key)
                .map(|(_, bounds)| [bounds[0], bounds[1]])
        };
        let Some(target_origin) = origin(key) else {
            return;
        };
        let world_bounds =
            (self.active_world != key.world).then(|| canvas::bounds(project, key, true));
        let level_bounds = canvas::bounds(project, key, false);
        let current_origin = if self.world {
            [0.0; 2]
        } else {
            self.selected.and_then(origin).unwrap_or([0.0; 2])
        };

        self.navigate_level(key, false);
        self.world = false;
        if let Some(world_bounds) = world_bounds {
            self.notify_world_view();
            self.camera_motion
                .fit_level_from_world(level_bounds, world_bounds, target_origin);
        } else {
            // Preserve the world-space camera position when switching local origins.
            for axis in 0..2 {
                self.viewport.center[axis] += current_origin[axis] - target_origin[axis];
            }
            self.camera_motion.fit(level_bounds, false);
        }
    }

    pub(super) fn select_search_destination(&mut self, destination: Destination) {
        let ctx = self.repaint.clone().unwrap_or_default();
        if self.defer_ui_action(
            &ctx,
            ui_commit::DeferredUiAction::Search(destination.clone()),
        ) {
            return;
        }
        match destination {
            Destination::World(world) => self.navigate_world(world),
            Destination::Definition(panel, uid) => {
                self.definition_uid = Some(uid);
                self.show_panel(Some(panel));
            }
            Destination::Level(key) => self.focus_search_level(key),
            Destination::LevelField(key) => {
                self.focus_search_level(key);
                self.show_panel(Some(Panel::Level));
            }
            Destination::Layer(key, layer) => {
                self.focus_search_level(key);
                self.switch_layer(layer);
            }
            Destination::Entity(iid) => {
                let target = self.project.as_ref().and_then(|p| {
                    p.level_keys().into_iter().find_map(|key| {
                        p.level(key)?["layerInstances"]
                            .as_array()?
                            .iter()
                            .enumerate()
                            .find_map(|(layer, li)| {
                                li["entityInstances"]
                                    .as_array()?
                                    .iter()
                                    .find(|e| e["iid"] == iid)
                                    .map(|e| (key, layer, e.clone()))
                            })
                    })
                });
                if let Some((key, layer, entity)) = target {
                    self.focus_search_level(key);
                    self.bleep_entity_effect(key, layer, &entity, None, 0.2, 5);
                }
            }
        }
    }

    #[cfg(test)]
    fn search_results(&self, query: &str) -> Vec<Entry> {
        let index = self.project.as_ref().map(search_index).unwrap_or_default();
        matching(&index, query).cloned().collect()
    }

    pub(super) fn palette_dialog(&mut self, ctx: &egui::Context) {
        if !self.command_palette || self.project.is_none() {
            self.command_palette = false;
            self.search_focus = false;
            self.search_entries = None;
            return;
        }
        if self.is_busy() {
            return;
        }
        let index = self
            .search_entries
            .get_or_insert_with(|| self.project.as_ref().map(search_index).unwrap_or_default());
        let id = egui::Id::new("command-palette");
        let width = 733.333_f32.min(ctx.content_rect().width() - 16.).max(1.);
        let modal = egui::Modal::new(id)
            .area(egui::Modal::default_area(id).anchor(Align2::CENTER_TOP, egui::vec2(0., 8.)))
            .frame(egui::Frame::NONE)
            .backdrop_color(Color32::from_black_alpha(179))
            .show(ctx, |ui| {
                ui.set_width(width);
                ui.spacing_mut().item_spacing = Vec2::ZERO;
                let input = ui.add_sized(
                    [width, 33.],
                    egui::TextEdit::singleline(&mut self.search)
                        .hint_text(locale::tr(ui, "Search for worlds, levels, entities etc."))
                        .font(FontId::proportional(14.667))
                        .margin(egui::Margin::symmetric(8, 5))
                        .background_color(theme::DARK)
                        .text_color(Color32::WHITE),
                );
                if !self.search_focus {
                    input.request_focus();
                    if let Some(mut state) = egui::text_edit::TextEditState::load(ctx, input.id) {
                        state
                            .cursor
                            .set_char_range(Some(egui::text::CCursorRange::two(
                                egui::text::CCursor::new(0),
                                egui::text::CCursor::new(self.search.chars().count()),
                            )));
                        state.store(ctx, input.id);
                    }
                    self.search_focus = true;
                } else if !input.has_focus() {
                    input.request_focus();
                }
                if input.changed() {
                    self.search_index = 0;
                    self.search_show_all = false;
                }
                let limit = if self.search_show_all {
                    usize::MAX
                } else {
                    PREVIEW_ROWS
                };
                let rows = matching(index, &self.search)
                    .take(limit)
                    .collect::<Vec<_>>();
                if rows.is_empty() {
                    return None;
                }

                let down = ctx.input(|i| i.key_pressed(Key::ArrowDown));
                let up = ctx.input(|i| i.key_pressed(Key::ArrowUp));
                self.search_index = (self.search_index + usize::from(down))
                    .saturating_sub(usize::from(up))
                    .min(rows.len() - 1);
                let more = !self.search_show_all && rows.len() == PREVIEW_ROWS;
                let content_height =
                    rows.len() as f32 * ROW_HEIGHT + 16. + if more { 37. } else { 0. };
                let height = content_height.min(ctx.content_rect().height() * 0.85);
                let mut selected = ctx
                    .input(|i| i.key_pressed(Key::Enter))
                    .then_some(self.search_index);
                ui.add_space(8.);
                egui::Frame::new()
                    .fill(theme::DARK)
                    .corner_radius(3.)
                    .show(ui, |ui| {
                        ui.set_height(height);
                        let mut scroll = egui::ScrollArea::vertical()
                            .max_height(height)
                            .auto_shrink([false, false]);
                        if input.changed() {
                            scroll = scroll.vertical_scroll_offset(0.);
                        }
                        scroll.show(ui, |ui| {
                            ui.add_space(8.);
                            for (row, entry) in rows.iter().enumerate() {
                                let response = result_row(ui, entry, row == self.search_index);
                                if response.clicked() {
                                    selected = Some(row);
                                }
                                if response.hovered() && ctx.input(|i| i.pointer.is_moving()) {
                                    self.search_index = row;
                                    ctx.request_repaint();
                                }
                                if row == self.search_index && (up || down) {
                                    response.scroll_to_me(None);
                                }
                            }
                            if more {
                                ui.add_space(2.);
                                let orange = Color32::from_rgb(227, 77, 0);
                                let button = egui::Button::new(
                                    RichText::new(locale::tr(ui, "SHOW ALL RESULTS"))
                                        .size(10.667)
                                        .color(orange),
                                )
                                .fill(theme::DARK)
                                .stroke(Stroke::new(1., orange))
                                .corner_radius(0.);
                                if ui.add_sized([ui.available_width(), 35.], button).clicked() {
                                    self.search_show_all = true;
                                    self.search_index = 0;
                                    ctx.request_repaint();
                                }
                            }
                            ui.add_space(8.);
                        });
                    });
                selected.map(|row| rows[row].destination.clone())
            });
        let close = modal.should_close();
        if close || modal.inner.is_some() {
            self.command_palette = false;
            self.search_focus = false;
            self.search_entries = None;
        }
        if !close && let Some(destination) = modal.inner {
            self.select_search_destination(destination);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn level_search_animates_from_current_camera_in_linear_world() {
        let mut project = Project::new();
        project.json["worldLayout"] = json!("LinearHorizontal");
        project.json["levels"][0]["pxWid"] = json!(256);
        let key = project.add_level(None, "Target", 256, 256).unwrap();
        for world_view in [false, true] {
            let mut app = EditorApp::empty();
            app.set_project(project.clone());
            app.world = world_view;
            app.viewport.center = [96.0, 105.0];
            app.viewport.zoom = 1.25;
            app.fit = false;

            app.select_search_destination(Destination::Level(key));

            assert_eq!(app.selected, Some(key));
            assert!(!app.world);
            assert!(!app.fit);
            assert_eq!(app.viewport.center, [-208.0, 105.0]);
            assert_eq!(app.viewport.zoom, 1.25);
            assert!(
                app.camera_motion
                    .tick(&mut app.viewport, [1140.0, 900.0], 1.0, 1, 1.0 / 60.0,)
            );
            assert!(app.viewport.center[0] > -208.0 && app.viewport.center[0] < 128.0);
            assert!(app.viewport.zoom > 1.25 && app.viewport.zoom < 900.0 / 336.0);
        }
    }

    #[test]
    fn cross_world_level_search_animates_from_destination_world_fit() {
        let mut project = Project::new();
        let world = Some(project.add_world("Destination").unwrap());
        let first = project.add_level(world, "Start", 256, 256).unwrap();
        let key = project.add_level(world, "Target", 256, 256).unwrap();
        project.move_level(first, -256, -200, 0).unwrap();
        project.move_level(key, 320, 50, 0).unwrap();
        let mut app = EditorApp::empty();
        app.set_project(project);
        app.viewport.center = [640.0, -470.0];
        app.viewport.zoom = 5.0;

        app.select_search_destination(Destination::Level(key));

        assert_eq!(app.active_world, world);
        assert!(matches!(&app.quick_notification, Some((name, _, _)) if name == "World view"));
        assert!(!app.fit);
        assert!(
            app.camera_motion
                .tick(&mut app.viewport, [1140.0, 900.0], 1.0, 1, 0.0)
        );
        assert_eq!(app.viewport.center, [-160.0, 3.0]);
        assert!((app.viewport.zoom - 1140.0 / (832.0 * 1.1 + 300.0)).abs() < 0.001);
    }

    #[test]
    fn searchable_array_items_match_individually_in_source_order() {
        let mut project = Project::new();
        let key = project.level_keys()[0];
        project.add_layer("Actors", "Entities", 16).unwrap();
        let def = project.add_entity_def("Player", 16, 16).unwrap();
        let field = project
            .add_field_definition(Some(def), "Loot", "String", true)
            .unwrap();
        project
            .update_field_definition(field, &json!({"searchable": true}))
            .unwrap();
        project.add_entity(key, 0, def, 16, 16).unwrap();
        let iid = project.layer(key, 0).unwrap()["entityInstances"][0]["iid"]
            .as_str()
            .unwrap()
            .to_owned();
        project
            .set_field_value(
                key,
                Some(&iid),
                field,
                json!(["Gold", "Silver", "Gold bar"]),
            )
            .unwrap();
        let mut app = EditorApp::empty();
        app.set_project(project);

        assert_eq!(
            app.search_results("gold")
                .iter()
                .map(|row| row.title.as_str())
                .collect::<Vec<_>>(),
            ["Player.\"Gold\"", "Player.\"Gold bar\""]
        );
        assert!(app.search_results("gold silver").is_empty());
        assert_eq!(app.search_results("player").len(), 2);
    }

    #[test]
    fn search_matches_chinese_project_identifiers_and_searchable_fields() {
        let mut project = Project::new();
        let key = project.level_keys()[0];
        let level = project.level_mut(key).unwrap();
        level["useAutoIdentifier"] = json!(false);
        level["identifier"] = "水晶洞穴".into();
        let field = project
            .add_field_definition(None, "Entrance", "String", false)
            .unwrap();
        project
            .update_field_definition(field, &json!({"searchable": true}))
            .unwrap();
        project
            .set_field_value(key, None, field, "北门、North".into())
            .unwrap();
        let mut app = EditorApp::empty();
        app.set_project(project);

        assert!(matches!(
            app.search_results("水晶").as_slice(),
            [row] if matches!(row.destination, Destination::Level(found) if found == key)
        ));
        assert!(matches!(
            app.search_results("north 北门").as_slice(),
            [row] if matches!(row.destination, Destination::LevelField(found) if found == key)
        ));
        assert!(app.search_results("north 南门").is_empty());
    }

    #[test]
    fn preview_154_search_finds_level_fields_and_instance_iids() {
        let mut p = Project::new();
        p.add_layer("Actors", "Entities", 16).unwrap();
        let entity = p.add_entity_def("Hero", 16, 16).unwrap();
        let field = p
            .add_field_definition(None, "Region", "String", false)
            .unwrap();
        p.update_field_definition(field, &serde_json::json!({"searchable":true}))
            .unwrap();
        let key = p.level_keys()[0];
        p.set_field_value(key, None, field, "Azure Delta".into())
            .unwrap();
        p.add_entity(key, 0, entity, 16, 16).unwrap();
        let level = p.level(key).unwrap();
        let level_iid = level["iid"].as_str().unwrap().to_owned();
        let layer_iid = level["layerInstances"][0]["iid"]
            .as_str()
            .unwrap()
            .to_owned();
        let entity_iid = level["layerInstances"][0]["entityInstances"][0]["iid"]
            .as_str()
            .unwrap()
            .to_owned();
        let mut app = EditorApp::empty();
        app.set_project(p);
        assert!(
            app.search_results("Azure Delta")
                .iter()
                .any(|row| matches!(&row.destination,Destination::LevelField(k) if *k==key))
        );
        assert!(
            app.search_results(&level_iid)
                .iter()
                .any(|row| matches!(&row.destination,Destination::Level(k) if *k==key))
        );
        assert!(
            app.search_results(&layer_iid)
                .iter()
                .any(|row| matches!(&row.destination,Destination::Layer(k,0) if *k==key))
        );
        assert!(
            app.search_results(&entity_iid)
                .iter()
                .any(|row| matches!(&row.destination,Destination::Entity(i) if *i==entity_iid))
        );
        let destination = app
            .search_results("Azure Delta")
            .into_iter()
            .find_map(|row| {
                matches!(row.destination, Destination::LevelField(_)).then_some(row.destination)
            })
            .unwrap();
        app.select_search_destination(destination);
        assert_eq!(app.selected, Some(key));
        assert!(app.panel == Some(Panel::Level));
        let destination = app
            .search_results(&layer_iid)
            .into_iter()
            .find_map(|row| {
                matches!(row.destination, Destination::Layer(..)).then_some(row.destination)
            })
            .unwrap();
        app.select_search_destination(destination);
        assert_eq!(app.layer, 0);
    }

    #[test]
    fn preview_154_show_all_results_and_preserve_last_query() {
        let mut p = Project::new();
        p.json["defs"]["entities"] = serde_json::json!(
            (0..25)
                .map(|i| serde_json::json!({"uid":100+i,"identifier":format!("Actor{i}")}))
                .collect::<Vec<_>>()
        );
        let mut app = EditorApp::empty();
        app.set_project(p);
        app.search = "Actor".into();
        app.command_palette = true;
        assert_eq!(app.search_results("Actor").len(), 25);
        let ctx = egui::Context::default();
        theme::configure(&ctx);
        let rect = Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000., 800.));
        let frame = |app: &mut EditorApp, events| {
            ctx.run(
                egui::RawInput {
                    screen_rect: Some(rect),
                    events,
                    ..Default::default()
                },
                |ctx| app.palette_dialog(ctx),
            )
        };
        frame(&mut app, vec![]);
        frame(
            &mut app,
            vec![egui::Event::PointerMoved(egui::pos2(500., 250.))],
        );
        frame(
            &mut app,
            vec![egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: egui::vec2(0., -1000.),
                modifiers: egui::Modifiers::NONE,
            }],
        );
        let out = frame(&mut app, vec![]);
        fn find(shape: &egui::Shape, label: &str) -> Option<Rect> {
            match shape {
                egui::Shape::Text(t) if t.galley.job.text == label => {
                    Some(t.galley.rect.translate(t.pos.to_vec2()))
                }
                egui::Shape::Vec(v) => v.iter().find_map(|shape| find(shape, label)),
                _ => None,
            }
        }
        assert!(
            out.shapes
                .iter()
                .any(|s| find(&s.shape, "Actor20").is_some())
        );
        assert!(
            out.shapes
                .iter()
                .all(|s| find(&s.shape, "Actor21").is_none())
        );
        let pos = out
            .shapes
            .iter()
            .find_map(|s| find(&s.shape, "SHOW ALL RESULTS"))
            .unwrap()
            .center();
        for pressed in [true, false] {
            frame(
                &mut app,
                vec![
                    egui::Event::PointerMoved(pos),
                    egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: Default::default(),
                    },
                ],
            );
        }
        assert!(app.search_show_all);
        let expanded = frame(&mut app, vec![]);
        assert!(
            expanded
                .shapes
                .iter()
                .any(|s| find(&s.shape, "Actor24").is_some())
        );
        app.command_palette = false;
        app.search_focus = false;
        let modifiers = egui::Modifiers {
            ctrl: true,
            command: true,
            ..Default::default()
        };
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(rect),
                modifiers,
                events: vec![egui::Event::Key {
                    key: Key::K,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers,
                }],
                ..Default::default()
            },
            |ctx| app.shortcuts(ctx),
        );
        assert!(app.command_palette);
        assert_eq!(app.search, "Actor");
        assert!(!app.search_show_all);
        frame(&mut app, vec![]);
        frame(&mut app, vec![egui::Event::Text("Z".into())]);
        assert_eq!(app.search, "Z");
    }

    #[test]
    fn source_search_includes_worlds_external_enums_and_token_categories_but_only_searchable_values()
     {
        let mut p = Project::new();
        p.enable_multiworlds().unwrap();
        p.json["worlds"][0]["identifier"] = "MainWorld".into();
        p.add_layer("Actors", "Entities", 16).unwrap();
        let uid = p.add_entity_def("Player", 16, 16).unwrap();
        let field = p
            .add_field_definition(Some(uid), "Label", "String", false)
            .unwrap();
        let key = p.level_keys()[0];
        p.add_entity(key, 0, uid, 16, 16).unwrap();
        let iid = p.layer(key, 0).unwrap()["entityInstances"][0]["iid"]
            .as_str()
            .unwrap()
            .to_owned();
        p.set_field_value(key, Some(&iid), field, "needle".into())
            .unwrap();
        p.json["defs"]["externalEnums"] =
            serde_json::json!([{"identifier":"Extern","uid":9000,"values":[]}]);
        let mut app = EditorApp::empty();
        app.set_project(p);
        assert!(matches!(
            app.search_results("world MainWorld")[0].destination,
            Destination::World(Some(0))
        ));
        assert!(
            app.search_results("definition Extern")
                .iter()
                .any(|row| matches!(
                    &row.destination,
                    Destination::Definition(Panel::Enums, 9000)
                ))
        );
        assert!(
            app.search_results("entity Player")
                .iter()
                .any(|row| matches!(&row.destination, Destination::Entity(_)))
        );
        assert!(app.search_results("needle").is_empty());
        app.project
            .as_mut()
            .unwrap()
            .update_field_definition(field, &serde_json::json!({"searchable":true}))
            .unwrap();
        assert!(
            app.search_results("entity needle")
                .iter()
                .any(|row| matches!(&row.destination, Destination::Entity(_)))
        );
        assert!(
            app.search_results("entity field needle")
                .iter()
                .any(|row| matches!(&row.destination, Destination::Entity(_)))
        );
        assert!(app.search_results("").is_empty());
    }
}
