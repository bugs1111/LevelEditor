use super::*;

#[derive(Clone)]
pub(super) enum Destination {
    World(Option<usize>),
    Level(LevelKey),
    Entity(String),
    Definition(Panel, i64),
}
fn keywords(raw: &str) -> String {
    raw.to_lowercase()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}
fn matches_query(raw: &str, query: &str) -> bool {
    let haystack = keywords(raw);
    keywords(query)
        .split_whitespace()
        .all(|part| haystack.contains(part))
}
impl EditorApp {
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
            Destination::Level(key) => {
                self.navigate_level(key, true);
                self.world = false;
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
                    self.navigate_level(key, true);
                    self.world = false;
                    self.bleep_entity_effect(key, layer, &entity, None, 0.2, 5);
                }
            }
        }
    }
    fn search_results(&self, query: &str) -> Vec<(String, Destination)> {
        if keywords(query).is_empty() {
            return Vec::new();
        }
        let Some(project) = &self.project else {
            return Vec::new();
        };
        let mut entries = Vec::new();
        for (kind, panel, category) in [
            ("layers", Panel::Layers, "layer"),
            ("entities", Panel::Entities, "entity"),
            ("enums", Panel::Enums, "enum"),
            ("externalEnums", Panel::Enums, "enum"),
            ("tilesets", Panel::Tilesets, "tileset"),
        ] {
            for def in project.json["defs"][kind].as_array().into_iter().flatten() {
                let name = text(def, "identifier", "");
                if matches_query(&format!("definition {category} {name}"), query)
                    && let Some(uid) = def["uid"].as_i64()
                {
                    entries.push((
                        format!("{category} definition · {name}"),
                        Destination::Definition(panel, uid),
                    ));
                }
            }
        }
        let worlds = project.json["worlds"]
            .as_array()
            .filter(|w| !w.is_empty())
            .map(|worlds| {
                worlds
                    .iter()
                    .enumerate()
                    .map(|(i, w)| (Some(i), text(w, "identifier", "World").to_owned()))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_else(|| vec![(None, "World".into())]);
        for (world, name) in worlds {
            if matches_query(&format!("world {name}"), query) {
                entries.push((format!("World · {name}"), Destination::World(world)));
            }
            for key in project
                .level_keys()
                .into_iter()
                .filter(|k| k.world == world)
            {
                let Some(level) = project.level(key) else {
                    continue;
                };
                let level_name = text(level, "identifier", "Level");
                if matches_query(&format!("level {name} {level_name}"), query) {
                    entries.push((
                        format!("Level · {name} / {level_name}"),
                        Destination::Level(key),
                    ));
                }
                for layer in level["layerInstances"].as_array().into_iter().flatten() {
                    for entity in layer["entityInstances"].as_array().into_iter().flatten() {
                        let id = text(entity, "__identifier", "Entity");
                        let def = project.json["defs"]["entities"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .find(|d| d["uid"] == entity["defUid"]);
                        let fields = entity["fieldInstances"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter(|field| {
                                !field["__value"].is_null()
                                    && def.and_then(|d| d["fieldDefs"].as_array()).is_some_and(
                                        |defs| {
                                            defs.iter().any(|d| {
                                                d["uid"] == field["defUid"]
                                                    && d["searchable"] == true
                                            })
                                        },
                                    )
                            })
                            .map(|f| {
                                f["__value"]
                                    .as_str()
                                    .map(str::to_owned)
                                    .unwrap_or_else(|| f["__value"].to_string())
                            })
                            .collect::<Vec<_>>()
                            .join(" ");
                        if matches_query(&format!("entity {id} {fields}"), query)
                            && let Some(iid) = entity["iid"].as_str()
                        {
                            entries.push((
                                format!("Entity · {level_name} / {id} {fields}"),
                                Destination::Entity(iid.into()),
                            ));
                        }
                    }
                }
            }
        }
        entries.truncate(20);
        entries
    }
    pub(super) fn palette_dialog(&mut self, ctx: &egui::Context) {
        if !self.command_palette {
            self.search_focus = false;
            return;
        }
        if self.is_busy() {
            return;
        }
        let mut entries = self.search_results(&self.search);
        let mut selected = None;
        let mut close = false;
        egui::Modal::new(egui::Id::new("command-palette")).show(ctx, |ui| {
            ui.set_width(570.0);
            ui.heading("Search worlds, levels, entities and commands");
            let input = ui.add(
                egui::TextEdit::singleline(&mut self.search)
                    .hint_text("Search…")
                    .desired_width(f32::INFINITY),
            );
            if !self.search_focus {
                input.request_focus();
                self.search_focus = true;
            }
            if input.changed() {
                self.search_index = 0;
            }
            if ctx.input(|i| i.key_pressed(Key::ArrowDown)) {
                self.search_index = (self.search_index + 1).min(entries.len().saturating_sub(1));
            }
            if ctx.input(|i| i.key_pressed(Key::ArrowUp)) {
                self.search_index = self.search_index.saturating_sub(1);
            }
            egui::ScrollArea::vertical()
                .max_height(440.0)
                .show(ui, |ui| {
                    for (index, (label, _)) in entries.iter().enumerate() {
                        let response = ui.selectable_label(self.search_index == index, label);
                        if response.clicked() {
                            selected = Some(index);
                        }
                        if index == self.search_index
                            && ctx.input(|i| {
                                i.key_pressed(Key::ArrowDown) || i.key_pressed(Key::ArrowUp)
                            })
                        {
                            response.scroll_to_me(Some(egui::Align::Center));
                        }
                    }
                });
            if ctx.input(|i| i.key_pressed(Key::Enter)) && !entries.is_empty() {
                selected = Some(self.search_index.min(entries.len() - 1));
            }
            close = ctx.input(|i| i.key_pressed(Key::Escape));
        });
        if let Some(index) = selected {
            self.command_palette = false;
            self.search_focus = false;
            self.select_search_destination(entries.swap_remove(index).1);
        }
        if close {
            self.command_palette = false;
            self.search_focus = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
            app.search_results("world MainWorld")[0].1,
            Destination::World(Some(0))
        ));
        assert!(
            app.search_results("enum Extern")
                .iter()
                .any(|(_, d)| matches!(d, Destination::Definition(Panel::Enums, 9000)))
        );
        assert!(
            app.search_results("entity Player")
                .iter()
                .any(|(_, d)| matches!(d, Destination::Entity(_)))
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
                .any(|(_, d)| matches!(d, Destination::Entity(_)))
        );
        assert!(app.search_results("").is_empty());
    }
}
