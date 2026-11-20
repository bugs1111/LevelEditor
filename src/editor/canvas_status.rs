use super::{selection_click::Element, tools::Gesture, *};

impl EditorApp {
    pub(super) fn draw_canvas_status(
        &self,
        ui: &egui::Ui,
        canvas_rect: Rect,
        response: &egui::Response,
        key: LevelKey,
    ) {
        if self.canvas_input_blocked(ui.ctx())
            || self.panel.is_some_and(|panel| panel != Panel::Entity)
            || !response.contains_pointer()
        {
            return;
        }
        let Some((pointer, project)) = ui
            .input(|input| input.pointer.hover_pos())
            .zip(self.project.as_ref())
        else {
            return;
        };
        let (position, world_position) = self.canvas_pointer_pixels(
            project,
            key,
            canvas_rect,
            pointer,
            ui.ctx().pixels_per_point(),
        );
        let coordinates = |label, [x, y]: [f32; 2]| {
            locale::format_ctx(
                ui.ctx(),
                label,
                &[
                    ("x", &(x as i64).to_string()),
                    ("y", &(y as i64).to_string()),
                ],
            )
        };
        let mut right = canvas_rect.right() - 3.;
        let top = canvas_rect.bottom() - 25.;
        let mut badge = |label: String, foreground: Color32, background: Color32| {
            let galley = ui
                .painter()
                .layout_no_wrap(label, FontId::proportional(12.), foreground);
            let left = right - galley.size().x - 16.;
            let rect = Rect::from_min_max(egui::pos2(left, top), egui::pos2(right, top + 22.));
            ui.painter().rect_filled(rect, 6., background);
            let text_pos = egui::pos2(left + 8., rect.center().y - galley.size().y / 2.);
            ui.painter().galley_with_override_text_color(
                text_pos + egui::vec2(0., 1.),
                galley.clone(),
                Color32::from_black_alpha(160),
            );
            ui.painter().galley(text_pos, galley, foreground);
            right = left - 8.;
        };
        badge(
            coordinates("World = {x},{y}", world_position),
            Color32::WHITE,
            Color32::from_rgba_unmultiplied(35, 51, 115, 102),
        );
        if !self.world {
            badge(
                coordinates("Level = {x},{y}", position),
                Color32::WHITE,
                Color32::from_rgba_unmultiplied(37, 115, 97, 102),
            );
            if let Some(layer) = project.layer(key, self.layer) {
                let local =
                    canvas::pointer_to_layer(project, key, self.layer, &self.viewport, position);
                let grid = number(layer, "__gridSize", 16.).max(1.);
                let cell = local.map(|axis| (axis / grid).floor());
                let mut label = coordinates("Grid = {x},{y}", cell);
                if let Some(Gesture::Rectangle { start, end, .. }) = self.gesture {
                    label.push_str(&format!(
                        " / {} x {}",
                        start[0].abs_diff(end[0]) + 1,
                        start[1].abs_diff(end[1]) + 1
                    ));
                }
                badge(
                    label,
                    Color32::WHITE,
                    Color32::from_rgba_unmultiplied(63, 195, 165, 102),
                );
            }
        }
        let element = if self.world {
            None
        } else {
            self.generic_element_at(position, self.single_layer)
                .and_then(|(index, cell, element)| {
                    status_element(ui.ctx(), project, project.layer(key, index)?, cell, element)
                })
        };
        let (label, color) = element.unwrap_or_else(|| ("--".into(), None));
        let (foreground, background) = color
            .map_or((Color32::WHITE, Color32::from_black_alpha(102)), |color| {
                (canvas::lighten(color, 0.66), canvas::darken(color, 0.5))
            });
        badge(label, foreground, background);
    }
}

fn status_element(
    ctx: &egui::Context,
    project: &Project,
    layer: &Value,
    cell: [i32; 2],
    element: Element,
) -> Option<(String, Option<Color32>)> {
    if let Element::Grid = element {
        if layer["__type"] == "Tiles" {
            let tiles = selection_click::tiles_at_cell(layer, cell)
                .filter_map(|tile| tile["t"].as_i64())
                .map(|id| id.to_string())
                .collect::<Vec<_>>();
            let format = if tiles.len() == 1 {
                "Tile {id}"
            } else {
                "Tiles {ids}"
            };
            let key = if tiles.len() == 1 { "id" } else { "ids" };
            return Some((
                locale::format_ctx(ctx, format, &[(key, &tiles.join(", "))]),
                None,
            ));
        }
        let width = layer["__cWid"].as_i64()?;
        let value = layer["intGridCsv"]
            .get((i64::from(cell[0]) + i64::from(cell[1]) * width) as usize)?
            .as_i64()?;
        let def = project.layer_def(layer["layerDefUid"].as_i64()?)?;
        let entry = canvas::values(&def["intGridValues"])
            .iter()
            .find(|entry| entry["value"] == value)?;
        let name = entry["identifier"]
            .as_str()
            .map_or_else(|| value.to_string(), |name| format!("{name} ({value})"));
        return Some((
            locale::format_ctx(ctx, "{name} (IntGrid)", &[("name", &name)]),
            entry["color"].as_str().map(theme::color),
        ));
    }
    let (index, field) = match element {
        Element::Entity(index) => (index, None),
        Element::Point { entity, field, .. } => (entity, Some(field)),
        Element::Grid => unreachable!(),
    };
    let entity = layer["entityInstances"].get(index)?;
    let def = canvas::values(&project.json["defs"]["entities"])
        .iter()
        .find(|def| def["uid"] == entity["defUid"])?;
    let name = def["identifier"].as_str()?;
    let label = if let Some(field) = field {
        let field = canvas::values(&def["fieldDefs"])
            .iter()
            .find(|def| def["uid"] == field)?["identifier"]
            .as_str()?;
        locale::format_ctx(
            ctx,
            "{name}.{field} (Entity point)",
            &[("name", name), ("field", field)],
        )
    } else {
        locale::format_ctx(ctx, "{name} (Entity)", &[("name", name)])
    };
    Some((
        label,
        Some(canvas::entity_smart_color(project, entity, def, false)),
    ))
}
