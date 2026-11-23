use super::{
    Scene, Transform, bitmap_font, darken, entities::entity_bounds, number, pair, text, values,
};
use crate::theme;
use eframe::egui::{self, Color32, Painter, Pos2, Rect, Stroke, Vec2, vec2};
use ldtk_core::{LevelKey, Project};
use serde_json::{Value, json};
use std::borrow::Cow;

fn text_width(ctx: &egui::Context, level: bool) -> f32 {
    // Heaps Text.maxWidth is in font pixels; the field root cancels DPI.
    (if level { 800. } else { 250. }) / ctx.pixels_per_point()
}

pub(super) fn field_value<'a>(owner: &'a Value, def: &Value) -> Cow<'a, Value> {
    if let Some(field) = values(&owner["fieldInstances"])
        .iter()
        .find(|f| f["defUid"] == def["uid"])
    {
        return Cow::Borrowed(&field["__value"]);
    }
    if def["isArray"] == true {
        return Cow::Owned(json!([]));
    }
    let value = def["defaultOverride"]["params"][0].clone();
    let kind = text(def, "__type", "");
    if kind == "Color"
        && let Some(n) = value.as_u64()
    {
        return Cow::Owned(json!(format!("#{:06X}", n & 0xffffff)));
    }
    if kind == "Tile"
        && let Some(s) = value.as_str()
    {
        return Cow::Owned(serde_json::from_str(s).unwrap_or(Value::Null));
    }
    if !value.is_null() {
        return Cow::Owned(value);
    }
    Cow::Owned(if def["canBeNull"] == true {
        Value::Null
    } else {
        match kind {
            "Int" | "Float" => json!(number(def, "min", 0.).max(0.)),
            "Bool" => json!(false),
            "String" | "Multilines" | "FilePath" => json!(""),
            "Color" => json!("#000000"),
            _ => Value::Null,
        }
    })
}
fn enum_definition<'a>(project: &'a Project, fd: &Value) -> Option<&'a Value> {
    let ty = text(fd, "__type", "");
    let ty = ty
        .strip_prefix("Array<")
        .and_then(|s| s.strip_suffix('>'))
        .unwrap_or(ty);
    let name = ty
        .strip_prefix("LocalEnum.")
        .or_else(|| ty.strip_prefix("ExternEnum."));
    let id = fd["type"]
        .as_str()
        .and_then(|s| s.strip_prefix("F_Enum("))
        .and_then(|s| s.strip_suffix(')'))
        .and_then(|s| s.parse::<i64>().ok());
    values(&project.json["defs"]["enums"])
        .iter()
        .chain(values(&project.json["defs"]["externalEnums"]))
        .find(|e| name.is_some_and(|n| e["identifier"] == n) || id.is_some_and(|id| e["uid"] == id))
}
pub(super) fn enum_value<'a>(project: &'a Project, fd: &Value, value: &Value) -> Option<&'a Value> {
    project.enum_value(
        enum_definition(project, fd)?["uid"].as_i64()?,
        value.as_str()?,
    )
}
pub(super) fn value_icon(project: &Project, fd: &Value, value: &Value) -> Option<Value> {
    if value.is_object() && value["tilesetUid"].is_i64() {
        return Some(value.clone());
    }
    let en = enum_definition(project, fd)?;
    let ev = enum_value(project, fd, value)?;
    let mut tile = ev["tileRect"].clone();
    if !tile.is_object() {
        return None;
    }
    if tile["tilesetUid"].is_null() {
        tile["tilesetUid"] = en["iconTilesetUid"].clone()
    }
    Some(tile)
}
fn displayed(owner: &Value, fd: &Value, world: bool) -> bool {
    if fd["editorDisplayMode"] == "Hidden" || (world && fd["editorShowInWorld"] == false) {
        return false;
    }
    if fd["editorAlwaysShow"] == true {
        return true;
    }
    let field = values(&owner["fieldInstances"])
        .iter()
        .find(|f| f["defUid"] == fd["uid"]);
    if fd["isArray"] == true {
        field.is_some_and(|f| !values(&f["__value"]).is_empty())
    } else {
        field.is_some_and(|f| !f["realEditorValues"][0].is_null())
    }
}

pub(super) fn level_has_beneath_label(project: &Project, level: &Value) -> bool {
    values(&level["fieldInstances"]).iter().any(|field| {
        values(&project.json["defs"]["levelFields"])
            .iter()
            .find(|fd| fd["uid"] == field["defUid"])
            .is_some_and(|fd| {
                fd["editorDisplayPos"] == "Beneath"
                    && matches!(
                        fd["editorDisplayMode"].as_str(),
                        Some("ValueOnly" | "NameAndValue" | "ArrayCountNoLabel" | "ArrayCountWithLabel")
                    )
                    && (fd["editorAlwaysShow"] == true || !field["realEditorValues"][0].is_null())
            })
    })
}

#[derive(Clone)]
enum Part {
    Text(String),
    Icon(Value),
    Color(Color32),
}
struct Row {
    label: String,
    parts: Vec<Part>,
    color: Color32,
    scale: f32,
    vertical: bool,
    wrap: bool,
    error: Option<String>,
    label_brighten: f32,
    value_brighten: f32,
}
fn full_brightness(c: Color32) -> Color32 {
    let m = c.r().max(c.g()).max(c.b()) as f32;
    if m == 0. {
        Color32::WHITE
    } else {
        Color32::from_rgba_unmultiplied(
            (c.r() as f32 * 255. / m).round() as u8,
            (c.g() as f32 * 255. / m).round() as u8,
            (c.b() as f32 * 255. / m).round() as u8,
            c.a(),
        )
    }
}
fn display_value(value: &Value, fd: &Value) -> String {
    if value.is_null() {
        return "null".into();
    }
    let kind = text(fd, "__type", "");
    let kind = kind
        .strip_prefix("Array<")
        .and_then(|s| s.strip_suffix('>'))
        .unwrap_or(kind);
    if kind == "Bool" && fd["editorDisplayMode"] == "ValueOnly" {
        return format!(
            "{}{}",
            if value == true { "+" } else { "-" },
            text(fd, "identifier", "")
        );
    }
    let raw = value
        .as_str()
        .map(str::to_owned)
        .unwrap_or_else(|| value.to_string());
    match kind {
        "Int" | "Float" => format!(
            "{}{}{}",
            text(fd, "editorTextPrefix", ""),
            raw,
            text(fd, "editorTextSuffix", "")
        ),
        "String" | "Multilines" | "FilePath" => format!("\"{raw}\""),
        "Point" => format!("{},{}", value["cx"], value["cy"]),
        "EntityRef" => format!("@({})", value["entityIid"].as_str().unwrap_or("null")),
        _ => raw,
    }
}
impl Scene<'_> {
    fn field_error(&self, owner: &Value, fd: &Value) -> Option<String> {
        let value = field_value(owner, fd);
        let array = fd["isArray"] == true;
        let items = if array {
            values(&value)
        } else {
            std::slice::from_ref(value.as_ref())
        };
        if array
            && fd["arrayMinLength"]
                .as_u64()
                .is_some_and(|n| items.len() < n as usize)
        {
            return Some("Array too short".into());
        }
        if array
            && fd["arrayMaxLength"]
                .as_u64()
                .is_some_and(|n| items.len() > n as usize)
        {
            return Some("Array too long".into());
        }
        let kind = text(fd, "__type", "")
            .trim_start_matches("Array<")
            .trim_end_matches('>');
        for item in items {
            if item.is_null() {
                if fd["canBeNull"] != true
                    && !(matches!(
                        kind,
                        "Int" | "Float" | "String" | "Multilines" | "Bool" | "Color"
                    ))
                {
                    return Some("Value required".into());
                }
                continue;
            }
            if kind == "FilePath"
                && let Some(path) = item.as_str()
            {
                let path = super::asset_path(self.project, path);
                let exists = *self
                    .file_states
                    .borrow_mut()
                    .entry(path.clone())
                    .or_insert_with(|| path.exists());
                if !exists {
                    return Some("File not found".into());
                }
            }
            if kind == "EntityRef" {
                let iid = item["entityIid"]
                    .as_str()
                    .or_else(|| item.as_str())
                    .unwrap_or("");
                let index = self
                    .entity_index
                    .get_or_init(|| EntityIndex::new(self.project));
                let Some((_, _, target)) = index.entities.get(iid) else {
                    return Some("Lost reference!".into());
                };
                let target_def = values(&self.project.json["defs"]["entities"])
                    .iter()
                    .find(|d| d["uid"] == target["defUid"])
                    .unwrap_or(&Value::Null);
                let name = text(
                    target_def,
                    "identifier",
                    text(target, "__identifier", "Entity"),
                );
                let wrong = match text(fd, "allowedRefs", "OnlySame") {
                    "OnlySame" => !owner["defUid"].is_null() && owner["defUid"] != target["defUid"],
                    "OnlySpecificEntity" => target["defUid"] != fd["allowedRefsEntityUid"],
                    "OnlyTags" => {
                        if !values(&target_def["tags"])
                            .iter()
                            .any(|t| values(&fd["allowedRefTags"]).contains(t))
                        {
                            return Some("Invalid ref tags".into());
                        }
                        false
                    }
                    _ => false,
                };
                if wrong {
                    return Some(format!("Invalid ref type {name}"));
                }
            }
        }
        None
    }
    fn row(&self, owner: &Value, fd: &Value, base: Color32) -> Option<Row> {
        if self.options.world && fd["editorShowInWorld"] == false {
            return None;
        }
        let error = self.field_error(owner, fd);
        if error.is_none() && !displayed(owner, fd, self.options.world) {
            return None;
        }
        let mode = text(fd, "editorDisplayMode", "Hidden");
        if error.is_none()
            && !matches!(
                mode,
                "NameAndValue" | "ValueOnly" | "ArrayCountWithLabel" | "ArrayCountNoLabel"
            )
        {
            return None;
        }
        let value = field_value(owner, fd);
        let array = fd["isArray"] == true;
        let items = if array {
            values(&value)
        } else {
            std::slice::from_ref(value.as_ref())
        };
        let color = fd["editorDisplayColor"]
            .as_str()
            .map(theme::color)
            .unwrap_or(base);
        let mut label = match mode {
            "NameAndValue" => format!(
                "{}{}",
                text(fd, "identifier", ""),
                if self.options.fields_table { "" } else { " =" }
            ),
            "ArrayCountWithLabel" => format!("{}:", text(fd, "identifier", "")),
            _ => String::new(),
        };
        if error.is_some() && mode != "NameAndValue" {
            label = format!("{}{}", text(fd, "identifier", ""), label);
        }
        let mut parts = Vec::new();
        if mode.starts_with("ArrayCount") {
            parts.push(Part::Text(format!("{} value(s)", items.len())))
        } else if matches!(mode, "NameAndValue" | "ValueOnly") {
            for (index, item) in items.iter().enumerate() {
                if item.is_null() && fd["editorAlwaysShow"] != true {
                    continue;
                }
                if let Some(icon) = value_icon(self.project, fd, item) {
                    parts.push(Part::Icon(icon))
                } else if text(fd, "__type", "").contains("Color") {
                    parts.push(Part::Color(theme::color(
                        item.as_str().unwrap_or("#000000"),
                    )))
                } else {
                    let mut value = if text(fd, "__type", "").contains("EntityRef") {
                        let iid = item["entityIid"]
                            .as_str()
                            .or_else(|| item.as_str())
                            .unwrap_or("");
                        let index = self
                            .entity_index
                            .get_or_init(|| EntityIndex::new(self.project));
                        match index.entities.get(iid) {
                            Some((key, _, entity)) => format!(
                                "@({} in {})",
                                text(entity, "__identifier", "Entity"),
                                self.project
                                    .level(*key)
                                    .map(|l| text(l, "identifier", "Level"))
                                    .unwrap_or("Level")
                            ),
                            None => "@(Lost reference!)".into(),
                        }
                    } else {
                        display_value(item, fd)
                    };
                    if fd["editorCutLongValues"] == true
                        && !(text(fd, "__type", "").contains("Bool") && mode == "ValueOnly")
                    {
                        value = value
                            .chars()
                            .take(70)
                            .collect::<String>()
                            .split('\n')
                            .take(2)
                            .collect::<Vec<_>>()
                            .join("\n");
                    }
                    if array && index + 1 < items.len() {
                        value.push(',')
                    }
                    parts.push(Part::Text(value));
                }
            }
            if array && items.is_empty() && fd["editorAlwaysShow"] == true {
                parts.push(Part::Text("--empty--".into()))
            }
        }
        let kind = text(fd, "__type", "");
        let vertical = array
            && ["String", "Multilines", "FilePath", "EntityRef"]
                .iter()
                .any(|k| kind.contains(k));
        if parts.is_empty() && error.is_none() {
            None
        } else {
            Some(Row {
                label,
                parts,
                color,
                scale: number(fd, "editorDisplayScale", 1.) * self.options.ui_scale,
                vertical,
                wrap: array && kind.contains("Point"),
                error,
                label_brighten: if mode == "NameAndValue" { 0.3 } else { 0. },
                value_brighten: if matches!(mode, "NameAndValue" | "ValueOnly") {
                    0.25
                } else {
                    0.
                },
            })
        }
    }
    fn part_size(&self, part: &Part, scale: f32, icon_size: f32, max_width: f32) -> Vec2 {
        match part {
            Part::Text(s) => bitmap_font::measure_flow(self.ctx, s, scale, max_width),
            Part::Icon(tile) => {
                let w = number(tile, "w", 16.);
                let h = number(tile, "h", 16.);
                vec2(w, h) * (icon_size / w.max(h).max(1.)) * scale
            }
            Part::Color(_) => vec2(icon_size, icon_size) * scale,
        }
    }
    fn parts_layout(&self, row: &Row, max_width: f32) -> (Vec2, Vec<(Vec2, Vec2)>) {
        let icon = if row.parts.len() > 1 { 16. } else { 24. };
        let mut positions = Vec::new();
        let mut pen = Vec2::ZERO;
        let mut size = Vec2::ZERO;
        let mut line_height = 0f32;
        for part in &row.parts {
            let p = self.part_size(part, 1., icon, max_width);
            if row.wrap && pen.x > 0. && pen.x + p.x > max_width {
                pen.x = 0.;
                pen.y += line_height;
                line_height = 0.;
            }
            positions.push((pen, p));
            size.x = size.x.max(pen.x + p.x);
            size.y = size.y.max(pen.y + p.y);
            line_height = line_height.max(p.y);
            if row.vertical {
                pen.y += p.y + 8. / self.ctx.pixels_per_point();
            } else {
                pen.x += p.x + 2. / self.ctx.pixels_per_point();
            }
        }
        if !row.vertical {
            // Horizontal h2d.Flow vertically centers each item on its line.
            for (pos, p) in &mut positions {
                if !row.wrap {
                    pos.y = (size.y - p.y) * 0.5;
                }
            }
        }
        if let Some(error) = &row.error {
            let err = bitmap_font::measure_flow(self.ctx, &format!("<{error}>"), 1., f32::INFINITY);
            let height = size.y.max(err.y);
            for (pos, _) in &mut positions {
                pos.x += err.x;
                pos.y += (height - size.y) * 0.5;
            }
            size.x += err.x;
            size.y = height;
        }
        (size, positions)
    }
    fn rows_size(&self, rows: &[Row], max_width: f32) -> (Vec2, f32, Vec<(Vec2, Vec2)>) {
        let mut max_label = 0f32;
        let mut max_value = 0f32;
        let mut sizes = Vec::new();
        for row in rows {
            let label = bitmap_font::measure_flow(self.ctx, &row.label, 1., max_width);
            let value = self.parts_layout(row, max_width).0;
            max_label = max_label.max(label.x);
            if self.options.fields_table || !row.label.is_empty() {
                max_value = max_value.max(value.x);
            }
            sizes.push((label, value));
        }
        let mut total = Vec2::ZERO;
        for (i, (row, (label, value))) in rows.iter().zip(&sizes).enumerate() {
            let width = if !row.label.is_empty() {
                max_label + 6. / self.ctx.pixels_per_point() + max_value
            } else if self.options.fields_table && max_label > 0. {
                max_label + 2. * self.options.ui_scale / self.ctx.pixels_per_point() + max_value
            } else {
                value.x
            };
            total.x = total.x.max(
                (width * row.scale * self.ctx.pixels_per_point()).ceil()
                    / self.ctx.pixels_per_point(),
            );
            total.y += ((label.y.max(value.y)
                + if !row.label.is_empty() {
                    2. / self.ctx.pixels_per_point()
                } else {
                    0.
                })
                * row.scale
                * self.ctx.pixels_per_point())
            .ceil()
                / self.ctx.pixels_per_point();
            if i + 1 < rows.len() && self.options.fields_table {
                total.y += (2. * self.options.ui_scale).round() / self.ctx.pixels_per_point();
            }
        }
        (total, max_label, sizes)
    }
    fn paint_rows(
        &mut self,
        painter: &Painter,
        rows: &[Row],
        anchor: Pos2,
        align_y: f32,
        max_width: f32,
        base_color: Color32,
    ) {
        let gpu_outline = !self.options.fields_table && super::outline::available();
        let opacity = painter.opacity();
        let mut unfiltered = painter.clone();
        if gpu_outline {
            unfiltered.set_opacity(1.);
        }
        let painter = &unfiltered;
        let max_width = max_width / self.options.ui_scale.max(0.001);
        let (size, label_width, sizes) = self.rows_size(rows, max_width);
        let min = anchor - vec2(size.x * 0.5, size.y * align_y);
        if self.options.fields_table && !rows.is_empty() {
            let background = |percent: u16| {
                let channel = |value: u8| ((u16::from(value) * percent + 50) / 100) as u8;
                let [r, g, b, a] = base_color.to_array();
                Color32::from_rgba_premultiplied(channel(r), channel(g), channel(b), a)
            };
            let pad = vec2(4. * self.options.ui_scale, 2. * self.options.ui_scale)
                / self.ctx.pixels_per_point();
            painter.rect_filled(
                Rect::from_min_size(
                    min - pad + vec2(0., 1. / self.ctx.pixels_per_point()),
                    size + pad * 2.,
                ),
                0.,
                background(35),
            );
            if label_width > 0. {
                painter.rect_filled(
                    Rect::from_min_size(
                        min - pad + vec2(0., 1. / self.ctx.pixels_per_point()),
                        vec2(
                            label_width * self.options.ui_scale + pad.x * 2.,
                            size.y + pad.y * 2.,
                        ),
                    ),
                    0.,
                    background(15),
                );
            }
        }
        let mut y = min.y;
        let density = self.ctx.pixels_per_point();
        for (row, (label_size, value_size)) in rows.iter().zip(sizes) {
            let content_height = label_size.y.max(value_size.y);
            let h = (content_height * row.scale * density).ceil() / density;
            // Flow centers its children in local font pixels before the
            // parent rounds the scaled row's occupied height.
            let center_y = |height: f32| {
                y + ((content_height - height) * 0.5 * density).trunc() * row.scale / density
            };
            if !row.label.is_empty() {
                let first = super::blur::begin(painter);
                let label_origin = egui::pos2(
                    min.x + (label_width - label_size.x) * row.scale,
                    center_y(label_size.y),
                );
                bitmap_font::paint_field(
                    self.ctx,
                    painter,
                    self.assets,
                    label_origin
                        - vec2(0., (2. * self.options.ui_scale).round() / self.ctx.pixels_per_point() * row.scale),
                    &row.label,
                    full_brightness(row.color.lerp_to_gamma(Color32::WHITE, row.label_brighten)),
                    row.scale,
                    max_width * row.scale,
                    (!self.options.fields_table && !gpu_outline).then_some(row.color),
                );
                if gpu_outline {
                    super::outline::finish(painter, first, darken(row.color, 0.66), opacity, label_origin, row.scale);
                }
            }
            let first = super::blur::begin(painter);
            let value_origin = egui::pos2(
                if row.label.is_empty() && !self.options.fields_table {
                    anchor.x - value_size.x * row.scale * 0.5
                } else {
                    min.x
                        + (label_width
                            + if label_width > 0. {
                                if row.label.is_empty() {
                                    2. * self.options.ui_scale / self.ctx.pixels_per_point()
                                } else {
                                    6. / self.ctx.pixels_per_point()
                                }
                            } else {
                                0.
                            })
                            * row.scale
                },
                center_y(value_size.y),
            );
            if let Some(error) = &row.error {
                let text = format!("<{error}>");
                let size = bitmap_font::measure_flow(self.ctx, &text, row.scale, f32::INFINITY);
                bitmap_font::paint_field(
                    self.ctx,
                    painter,
                    self.assets,
                    value_origin
                        + vec2(
                            0.,
                            (value_size.y * row.scale - size.y) * 0.5
                                - (2. * self.options.ui_scale).round()
                                    / self.ctx.pixels_per_point()
                                    * row.scale,
                        ),
                    &text,
                    Color32::from_rgb(255, 68, 0),
                    row.scale,
                    f32::INFINITY,
                    (!self.options.fields_table && !gpu_outline).then_some(row.color),
                );
            }
            let (_, positions) = self.parts_layout(row, max_width);
            for (part, (offset, part_size)) in row.parts.iter().zip(positions) {
                let p = value_origin + offset * row.scale;
                let size = part_size * row.scale;
                match part {
                    Part::Text(value) => {
                        bitmap_font::paint_field(
                            self.ctx,
                            painter,
                            self.assets,
                            p - vec2(
                                0.,
                                (2. * self.options.ui_scale).round() / self.ctx.pixels_per_point()
                                    * row.scale,
                            ),
                            value,
                            full_brightness(
                                row.color.lerp_to_gamma(Color32::WHITE, row.value_brighten),
                            ),
                            row.scale,
                            max_width * row.scale,
                            (!self.options.fields_table && !gpu_outline).then_some(row.color),
                        );
                    }
                    Part::Color(color) => {
                        painter.circle(
                            p + size * 0.5,
                            size.x * 0.5,
                            *color,
                            Stroke::new(1.0_f32, Color32::BLACK.gamma_multiply(0.8)),
                        );
                    }
                    Part::Icon(tile) => {
                        if let Some(atlas) = tile["tilesetUid"]
                            .as_i64()
                            .and_then(|id| self.project.tileset(id))
                            && let Some(texture) = self.texture(atlas)
                        {
                            let source = Rect::from_min_size(
                                [number(tile, "x", 0.), number(tile, "y", 0.)].into(),
                                vec2(number(tile, "w", 16.), number(tile, "h", 16.)),
                            );
                            painter.image(
                                texture.id(),
                                Rect::from_min_size(p, size),
                                Rect::from_min_max(
                                    (source.min.to_vec2() / texture.size_vec2()).to_pos2(),
                                    (source.max.to_vec2() / texture.size_vec2()).to_pos2(),
                                ),
                                Color32::WHITE,
                            );
                        }
                    }
                }
            }
            if gpu_outline {
                super::outline::finish(painter, first, darken(row.color, 0.66), opacity, value_origin, row.scale);
            }
            y +=
                h + if row.label.is_empty() {
                    0.
                } else {
                    2. * row.scale / self.ctx.pixels_per_point()
                } + if self.options.fields_table {
                    (2. * self.options.ui_scale).round() / self.ctx.pixels_per_point()
                } else {
                    0.
                };
        }
    }
    #[allow(clippy::too_many_arguments)]
    pub(super) fn entity_fields(
        &mut self,
        painter: &Painter,
        entity: &Value,
        def: &Value,
        layer_def: &Value,
        transform: Transform,
        color: Color32,
        key: LevelKey,
        alpha: f32,
        active: bool,
    ) {
        let bounds = entity_bounds(entity, def);
        let rect = self.screen(transform.rect(bounds));
        let mut text_painter = painter.clone();
        text_painter.multiply_opacity(alpha);
        let mut name_height = 0.;
        if def["showName"] == true && rect.expand(60.).intersects(self.canvas) {
            let name = text(def, "identifier", text(entity, "__identifier", "Entity"))
                .chars()
                .take(16)
                .collect::<String>();
            let size = bitmap_font::measure(self.ctx, &name, 1., f32::INFINITY);
            name_height = size.y;
            let gpu_outline = !self.options.fields_table && super::outline::available();
            let mut name_painter = text_painter.clone();
            if gpu_outline {
                name_painter.set_opacity(1.);
            }
            let first = super::blur::begin(&name_painter);
            let name_color = self.smart_color_bright(entity, def, true);
            let name_origin = rect.center_top() - vec2(size.x * 0.5, size.y);
            bitmap_font::paint(
                self.ctx,
                &name_painter,
                self.assets,
                name_origin,
                &name,
                name_color,
                1.,
                f32::INFINITY,
                !self.options.fields_table && !gpu_outline,
            );
            if gpu_outline {
                super::outline::finish(
                    &name_painter,
                    first,
                    darken(name_color, 0.66),
                    text_painter.opacity(),
                    name_origin,
                    1.,
                );
            }
        }
        let fields = values(&def["fieldDefs"]);
        if rect
            .expand(300. * self.options.ui_scale)
            .intersects(self.canvas)
        {
            for pos in ["Above", "Center", "Beneath"] {
                let group = fields
                    .iter()
                    .filter(|fd| text(fd, "editorDisplayPos", "Above") == pos)
                    .collect::<Vec<_>>();
                let color = if group
                    .iter()
                    .any(|fd| self.field_error(entity, fd).is_some())
                {
                    Color32::RED
                } else {
                    color
                };
                let rows = group
                    .into_iter()
                    .filter_map(|fd| self.row(entity, fd, color))
                    .collect::<Vec<_>>();
                let (anchor, align) = match pos {
                    "Above" => (rect.center_top() - vec2(0., name_height), 1.),
                    "Center" => (rect.center(), 0.5),
                    _ => (rect.center_bottom(), 0.),
                };
                self.paint_rows(
                    &text_painter,
                    &rows,
                    anchor,
                    align,
                    text_width(self.ctx, false) * self.options.ui_scale,
                    color,
                );
            }
        }
        if let Some(iid) = entity["iid"].as_str() {
            let index = self
                .entity_index
                .get_or_init(|| EntityIndex::new(self.project));
            if let Some(refs) = index.incoming.get(iid) {
                for &(source_key, source_layer, source, fd) in refs {
                    if source_key == key {
                        continue;
                    }
                    let source_def = values(&self.project.json["defs"]["entities"])
                        .iter()
                        .find(|d| d["uid"] == source["defUid"])
                        .unwrap_or(&Value::Null);
                    let center = fd["editorDisplayMode"] == "RefLinkBetweenCenters";
                    let source_point = if center {
                        entity_bounds(source, source_def).center().into()
                    } else {
                        pair(&source["px"], [0.; 2])
                    };
                    let target_point = if center {
                        bounds.center().into()
                    } else {
                        pair(&entity["px"], [0.; 2])
                    };
                    let source_level = self.project.level(source_key).unwrap();
                    let target_level = self.project.level(key).unwrap();
                    let world_origin = [
                        number(source_level, "worldX", 0.) - number(target_level, "worldX", 0.),
                        number(source_level, "worldY", 0.) - number(target_level, "worldY", 0.),
                    ];
                    let start = self.point(transform.point([
                        source_point[0]
                            + world_origin[0]
                            + number(source_layer, "__pxTotalOffsetX", 0.)
                            - number(layer_def, "pxOffsetX", 0.),
                        source_point[1]
                            + world_origin[1]
                            + number(source_layer, "__pxTotalOffsetY", 0.)
                            - number(layer_def, "pxOffsetY", 0.),
                    ]));
                    let end = self.point(transform.point(target_point));
                    let cut = source_key.world != key.world
                        || source_level["worldDepth"] != target_level["worldDepth"];
                    render_link(
                        painter,
                        start,
                        end,
                        self.smart_color(source, source_def),
                        text(fd, "editorLinkStyle", "CurvedArrow"),
                        self.view.zoom,
                        if cut {
                            LinkEnding::CutTarget
                        } else {
                            LinkEnding::Full
                        },
                    );
                }
            }
        }
        let graphic_alpha = alpha
            * if active {
                1.
            } else {
                number(layer_def, "inactiveOpacity", 1.)
            };
        for fd in fields {
            if displayed(entity, fd, self.options.world) {
                let color = if fields
                    .iter()
                    .filter(|other| {
                        text(other, "editorDisplayPos", "Above")
                            == text(fd, "editorDisplayPos", "Above")
                    })
                    .any(|other| self.field_error(entity, other).is_some())
                {
                    Color32::RED
                } else {
                    color
                };
                self.field_graphics(
                    painter,
                    entity,
                    def,
                    layer_def,
                    fd,
                    transform,
                    color.gamma_multiply(graphic_alpha),
                    key,
                );
            }
        }
    }
    pub(super) fn level_color(&self, level: &Value) -> Color32 {
        level_smart_color(self.project, level)
    }
    pub(super) fn level_fields(
        &mut self,
        painter: &Painter,
        level: &Value,
        origin: [f32; 2],
        title_height: f32,
    ) {
        let bounds = self.screen([
            origin[0],
            origin[1],
            origin[0] + number(level, "pxWid", 256.),
            origin[1] + number(level, "pxHei", 256.),
        ]);
        let defs = values(&self.project.json["defs"]["levelFields"]);
        let color = if defs.iter().any(|fd| self.field_error(level, fd).is_some()) {
            Color32::RED
        } else {
            let [r, g, b] = self.project.level_smart_color(level, true);
            Color32::from_rgb(r, g, b)
        };
        let mut rows = defs
            .iter()
            .filter_map(|fd| self.row(level, fd, color))
            .collect::<Vec<_>>();
        let max_width = text_width(self.ctx, true);
        let size = self.rows_size(&rows, max_width).0;
        if size.x <= 0. || size.y <= 0. {
            return;
        }
        let layout = self
            .options
            .selected
            .world
            .and_then(|i| self.project.json["worlds"].get(i))
            .unwrap_or(&self.project.json);
        let layout = text(layout, "worldLayout", "Free");
        let fit = if self.options.world && layout == "LinearHorizontal" {
            bounds.width() / size.x
        } else if self.options.world && layout == "LinearVertical" {
            bounds.height() / size.y
        } else {
            (bounds.width() / size.x).min(bounds.height() / size.y)
        }
        .min(1.);
        for row in &mut rows {
            row.scale *= fit;
        }
        let mut painter = painter.clone();
        if self.options.world {
            let backing_zoom = self.view.zoom * self.ctx.pixels_per_point();
            painter.multiply_opacity(((backing_zoom - 0.1) / 0.1).clamp(0., 1.));
        }
        let (anchor, align) = if self.options.world {
            let [x, y] = origin;
            let width = number(level, "pxWid", 256.);
            let height = number(level, "pxHei", 256.);
            let display_size = size * fit;
            let level_size = display_size / self.view.zoom;
            let left = (x + width * 0.5).round() - level_size.x * 0.5;
            let position = match layout {
                "LinearHorizontal" => [left, y + height + 32.],
                // WorldRender adds worldX twice in this layout.
                "LinearVertical" => [2. * x + width + 32., (y + height * 0.5).round() - level_size.y * 0.5],
                _ => [left, y + height - level_size.y],
            };
            (
                self.level_annotation_point(position.map(f32::trunc)) + vec2(display_size.x * 0.5, 0.),
                0.,
            )
        } else {
            let size = size * fit;
            let zoom = self.view.zoom;
            let center = (number(level, "worldX", 0.) + number(level, "pxWid", 256.) * 0.5).round();
            let left = (center - size.x * 0.5 / zoom).trunc();
            let top = (number(level, "worldY", 0.) - 12. - size.y / zoom).trunc()
                - title_height / zoom
                - 3. * self.options.ui_scale;
            (
                self.level_annotation_point([left, top]) + vec2(size.x * 0.5, 0.),
                0.,
            )
        };
        self.paint_rows(
            &painter,
            &rows,
            anchor,
            align,
            max_width * self.options.ui_scale,
            color,
        );
    }
    #[allow(clippy::too_many_arguments)]
    pub(super) fn field_graphics(
        &self,
        painter: &Painter,
        entity: &Value,
        def: &Value,
        layer_def: &Value,
        fd: &Value,
        transform: Transform,
        base: Color32,
        key: LevelKey,
    ) {
        let mode = text(fd, "editorDisplayMode", "Hidden");
        let color = fd["editorDisplayColor"]
            .as_str()
            .map(theme::color)
            .unwrap_or(base);
        let value = field_value(entity, fd);
        let items = if fd["isArray"] == true {
            values(&value)
        } else {
            std::slice::from_ref(value.as_ref())
        };
        let grid = number(layer_def, "gridSize", 16.).max(1.);
        let pivot = pair(&entity["px"], [0.; 2]);
        if mode == "RadiusPx" || mode == "RadiusGrid" {
            let radius =
                value.as_f64().unwrap_or(0.) as f32 * if mode == "RadiusGrid" { grid } else { 1. };
            if radius > 0. {
                painter.circle_stroke(
                    self.point(transform.point(pivot)),
                    radius * transform.scale * self.view.zoom,
                    Stroke::new(2.0_f32, color.gamma_multiply(0.33)),
                );
            }
            return;
        }
        if mode.starts_with("RefLinkBetween") {
            let start = if mode == "RefLinkBetweenCenters" {
                entity_bounds(entity, def).center().into()
            } else {
                pivot
            };
            let start = self.point(transform.point(start));
            for reference in items {
                let Some(iid) = reference["entityIid"]
                    .as_str()
                    .or_else(|| reference.as_str())
                else {
                    continue;
                };
                let Some((target_key, target_layer, target)) = self
                    .entity_index
                    .get_or_init(|| EntityIndex::new(self.project))
                    .entities
                    .get(iid)
                    .copied()
                else {
                    continue;
                };
                let target_def = values(&self.project.json["defs"]["entities"])
                    .iter()
                    .find(|d| d["uid"] == target["defUid"])
                    .unwrap_or(&Value::Null);
                let local = if mode == "RefLinkBetweenCenters" {
                    entity_bounds(target, target_def).center().into()
                } else {
                    pair(&target["px"], [0.; 2])
                };
                let from = self.project.level(key).unwrap();
                let to = self.project.level(target_key).unwrap();
                let selected = self.project.level(self.options.selected).unwrap_or(from);
                let target_origin = if self.options.world {
                    [number(to, "worldX", 0.), number(to, "worldY", 0.)]
                } else {
                    [
                        number(to, "worldX", 0.) - number(selected, "worldX", 0.),
                        number(to, "worldY", 0.) - number(selected, "worldY", 0.),
                    ]
                };
                let target_ld = self
                    .project
                    .layer_def(target_layer["layerDefUid"].as_i64().unwrap_or(-1))
                    .unwrap_or(&Value::Null);
                let target_transform = self
                    .layer_transform(to, target_layer, target_ld, target_origin)
                    .offset([
                        number(target_layer, "__pxTotalOffsetX", 0.),
                        number(target_layer, "__pxTotalOffsetY", 0.),
                    ]);
                let end = self.point(target_transform.point(local));
                let cut = key.world != target_key.world || from["worldDepth"] != to["worldDepth"];
                render_link(
                    painter,
                    start,
                    end,
                    color,
                    text(fd, "editorLinkStyle", "CurvedArrow"),
                    self.view.zoom,
                    if cut {
                        LinkEnding::CutOrigin
                    } else {
                        LinkEnding::Full
                    },
                );
            }
            return;
        }
        if !matches!(mode, "Points" | "PointStar" | "PointPath" | "PointPathLoop") {
            return;
        }
        let b = entity_bounds(entity, def);
        let origin = [
            if def["resizableX"] == true {
                b.center().x
            } else {
                ((pivot[0]
                    - if number(def, "pivotX", 0.) == 1. {
                        1.
                    } else {
                        0.
                    })
                    / grid)
                    .trunc()
                    * grid
                    + grid * 0.5
            },
            if def["resizableY"] == true {
                b.center().y
            } else {
                ((pivot[1]
                    - if number(def, "pivotY", 0.) == 1. {
                        1.
                    } else {
                        0.
                    })
                    / grid)
                    .trunc()
                    * grid
                    + grid * 0.5
            },
        ];
        let start = self.point(transform.point(origin));
        let mut previous = start;
        for point in items {
            if point.is_null() {
                continue;
            }
            let target = self.point(transform.point([
                (number(point, "cx", 0.) + 0.5) * grid,
                (number(point, "cy", 0.) + 0.5) * grid,
            ]));
            if mode != "Points" {
                render_link(
                    painter,
                    previous,
                    target,
                    color,
                    text(fd, "editorLinkStyle", "CurvedArrow"),
                    self.view.zoom,
                    LinkEnding::None,
                );
            }
            let r = 4. * transform.scale * self.view.zoom;
            painter.add(egui::Shape::convex_polygon(
                vec![
                    target + vec2(0., -r),
                    target + vec2(r, 0.),
                    target + vec2(0., r),
                    target + vec2(-r, 0.),
                ],
                darken(color, 0.6),
                Stroke::new(1.0_f32, color.gamma_multiply(0.66)),
            ));
            if mode == "PointPath" || mode == "PointPathLoop" {
                previous = target;
            }
        }
        if mode == "PointPathLoop" && items.len() > 1 {
            render_link(
                painter,
                previous,
                start,
                color,
                text(fd, "editorLinkStyle", "CurvedArrow"),
                self.view.zoom,
                LinkEnding::None,
            );
        }
    }
}
pub(super) struct EntityIndex<'a> {
    entities: std::collections::HashMap<&'a str, (LevelKey, &'a Value, &'a Value)>,
    incoming: std::collections::HashMap<&'a str, Vec<(LevelKey, &'a Value, &'a Value, &'a Value)>>,
}
impl<'a> EntityIndex<'a> {
    pub(super) fn new(project: &'a Project) -> Self {
        let mut index = Self {
            entities: Default::default(),
            incoming: Default::default(),
        };
        for key in project.level_keys() {
            let Some(level) = project.level(key) else {
                continue;
            };
            for layer in values(&level["layerInstances"]) {
                for entity in values(&layer["entityInstances"]) {
                    if let Some(iid) = entity["iid"].as_str() {
                        index.entities.insert(iid, (key, layer, entity));
                    }
                    let Some(def) = values(&project.json["defs"]["entities"])
                        .iter()
                        .find(|d| d["uid"] == entity["defUid"])
                    else {
                        continue;
                    };
                    for fd in values(&def["fieldDefs"])
                        .iter()
                        .filter(|f| text(f, "editorDisplayMode", "").starts_with("RefLink"))
                    {
                        if let Some(field) = values(&entity["fieldInstances"])
                            .iter()
                            .find(|f| f["defUid"] == fd["uid"])
                        {
                            let v = &field["__value"];
                            for value in if fd["isArray"] == true {
                                values(v)
                            } else {
                                std::slice::from_ref(v)
                            } {
                                if let Some(iid) =
                                    value["entityIid"].as_str().or_else(|| value.as_str())
                                {
                                    index
                                        .incoming
                                        .entry(iid)
                                        .or_default()
                                        .push((key, layer, entity, fd));
                                }
                            }
                        }
                    }
                }
            }
        }
        index
    }
}

/// Source temporary links use the same geometry and Full endpoints as stored links.
pub(crate) fn draw_entity_ref_link(
    painter: &Painter,
    start: Pos2,
    end: Pos2,
    color: Color32,
    style: &str,
    zoom: f32,
) {
    render_link(painter, start, end, color, style, zoom, LinkEnding::Full);
}

#[derive(Clone, Copy, PartialEq)]
enum LinkEnding {
    None,
    Full,
    CutOrigin,
    CutTarget,
}
fn render_link(
    painter: &Painter,
    mut start: Pos2,
    mut end: Pos2,
    color: Color32,
    style: &str,
    zoom: f32,
    ending: LinkEnding,
) {
    let len = start.distance(end);
    if len < 0.001 {
        return;
    }
    let dir = (end - start) / len;
    let normal = vec2(-dir.y, dir.x);
    if ending == LinkEnding::CutOrigin {
        end = start + dir * len.min(60. * zoom);
        painter.line_segment(
            [end - normal * 4. * zoom, end + normal * 4. * zoom],
            Stroke::new(1.0_f32, color.gamma_multiply(0.5)),
        );
    } else if ending == LinkEnding::CutTarget {
        start = end - dir * len.min(60. * zoom);
        painter.line_segment(
            [start - normal * 4. * zoom, start + normal * 4. * zoom],
            Stroke::new(1.0_f32, color),
        );
    } else if ending == LinkEnding::Full
        && matches!(style, "ZigZag" | "DashedLine" | "ArrowsLine")
        && len >= 12. * zoom
    {
        start += dir * 3. * zoom;
        end -= dir * 4. * zoom;
    } else if ending == LinkEnding::Full && len >= 20. * zoom {
        end -= dir * 4. * zoom;
    }
    let len = start.distance(end);
    let count = (len / (4. * zoom.max(0.01))).clamp(8., 30.);
    let dash = len / count;
    let mut previous = start;
    let mut last_dir = dir;
    if style == "ArrowsLine" {
        let count = len / 16.;
        for n in 0..count.ceil() as usize {
            let r = n as f32 / (count - 1.);
            let p = start + dir * n as f32 * 16.;
            let size = 9.;
            let left = (-dir + normal) * size * std::f32::consts::FRAC_1_SQRT_2;
            let right = (-dir - normal) * size * std::f32::consts::FRAC_1_SQRT_2;
            let stroke = Stroke::new(
                (4. - 2. * r).max(0.),
                color.gamma_multiply((1. - 0.5 * r).max(0.)),
            );
            painter.line_segment([p + left, p], stroke);
            painter.line_segment([p, p + right], stroke);
        }
        return;
    }
    for n in 0..count.ceil() as usize {
        let r = n as f32 / (count - 1.).max(1.);
        let mut next = start + dir * (n as f32 * dash);
        match style {
            "ZigZag" => {
                next += normal
                    * if n % 2 == 0 { 1. } else { -1. }
                    * 2.1
                    * zoom
                    * (1. - r)
                    * (r / 0.05).min(1.);
                painter.line_segment(
                    [previous, next],
                    Stroke::new(2. - r, color.gamma_multiply(1. - 0.7 * r)),
                );
            }
            "DashedLine" => {
                painter.line_segment(
                    [previous, previous + dir * dash * 0.6],
                    Stroke::new(4. - 2. * r, color.gamma_multiply(1. - 0.6 * r)),
                );
            }
            _ => {
                if style == "CurvedArrow" {
                    next += normal
                        * (len / zoom.max(0.01) / 200.).clamp(2., 15.)
                        * zoom
                        * (r * std::f32::consts::PI).sin();
                }
                painter.line_segment(
                    [previous, next],
                    Stroke::new(1. + r * 3., color.gamma_multiply(0.4 + 0.6 * r)),
                );
            }
        }
        if next.distance(previous) > 0.001 {
            last_dir = (next - previous).normalized();
        }
        previous = next;
    }
    if style == "ZigZag" {
        painter.line_segment(
            [previous, end],
            Stroke::new(1.0_f32, color.gamma_multiply(0.3)),
        );
    }
    if style == "CurvedArrow" || style == "StraightArrow" {
        let side = vec2(-last_dir.y, last_dir.x);
        let size = if len <= 32. * zoom { 10. } else { 12. };
        let ca = (std::f32::consts::PI * 0.8).cos();
        let sa = (std::f32::consts::PI * 0.8).sin();
        painter.add(egui::Shape::convex_polygon(
            vec![
                previous + (last_dir * ca + side * sa) * size,
                previous + last_dir * 2.,
                previous + (last_dir * ca - side * sa) * size,
            ],
            color,
            Stroke::NONE,
        ));
    }
}

pub(crate) fn level_smart_color(project: &Project, level: &Value) -> Color32 {
    let [r, g, b] = project.level_smart_color(level, false);
    Color32::from_rgb(r, g, b)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn source_flow_measures_last_line_baseline_not_font_line_height() {
        let ctx = egui::Context::default();
        ctx.set_pixels_per_point(2.);
        let _ = ctx.run(egui::RawInput::default(), |_| {});
        assert_eq!(
            bitmap_font::measure_flow(&ctx, "10 HP", 1., f32::INFINITY),
            vec2(23.5, 10.5)
        );
        assert_eq!(
            bitmap_font::measure_flow(&ctx, "10 HP\n10 HP", 1., f32::INFINITY),
            vec2(23.5, 23.5)
        );
    }
    #[test]
    fn source_local_enum_lookup_cleans_legacy_identifiers_but_external_is_literal() {
        let mut project = Project::new();
        project.json["identifierStyle"] = json!("Capitalize");
        project.json["defs"]["enums"] = json!([{"uid":5,"identifier":"ItemType","externalRelPath":null,"iconTilesetUid":99,"values":[{"id":"Sword","tileRect":{"x":16,"y":432,"w":16,"h":16}},{"id":"Heavy_sword","tileRect":{"x":80,"y":432,"w":16,"h":16}}]}]);
        let fd = json!({"__type":"Array<LocalEnum.ItemType>","type":"F_Enum(5)"});
        assert_eq!(
            enum_value(&project, &fd, &json!("Heavy sword")).unwrap()["id"],
            "Heavy_sword"
        );
        assert_eq!(
            value_icon(&project, &fd, &json!("Heavy sword")).unwrap()["x"],
            80
        );
        project.json["defs"]["enums"][0]["externalRelPath"] = json!("external.cdb");
        assert!(enum_value(&project, &fd, &json!("Heavy sword")).is_none());
    }
    #[test]
    fn source_field_width_is_in_bitmap_pixels_at_retina_dpi() {
        let ctx = egui::Context::default();
        ctx.set_pixels_per_point(2.);
        let _ = ctx.run(egui::RawInput::default(), |_| {});
        assert_eq!(text_width(&ctx, false), 125.);
        assert_eq!(text_width(&ctx, true), 400.);
        let text = "\"A door to the right was opened by some hidden mechanism.\"";
        let size = bitmap_font::measure(&ctx, text, 1., text_width(&ctx, false));
        let line = bitmap_font::measure(&ctx, "A", 1., f32::INFINITY).y;
        assert_eq!(size.y, line * 3.);
    }
    #[test]
    fn validation_labels_match_installed_original_js_vectors() {
        let oracle: Value =
            serde_json::from_str(include_str!("../docs/status/renderer-oracle.json")).unwrap();
        let mut project = Project::new();
        project.json["defs"]["entities"] =
            json!([{"uid":1,"identifier":"Owner"},{"uid":2,"identifier":"Target","tags":[]}]);
        project.json["levels"] =
            json!([{"layerInstances":[{"entityInstances":[{"iid":"exists","defUid":2}]}]}]);
        let ctx = egui::Context::default();
        let view = crate::viewport::Viewport::default();
        let hidden = std::collections::HashSet::new();
        let options = super::super::render_tests::options(
            &hidden,
            LevelKey {
                world: None,
                level: 0,
            },
        );
        let mut assets = crate::assets::Assets::default();
        let scene = Scene {
            ctx: &ctx,
            project: &project,
            view: &view,
            canvas: Rect::EVERYTHING,
            projection_origin: Rect::EVERYTHING.min.into(),
            options: &options,
            assets: &mut assets,
            entity_index: Default::default(),
            file_states: Default::default(),
        };
        for case in values(&oracle["fields"]) {
            let owner = json!({"defUid":1,"fieldInstances":[{"defUid":3,"__value":case["value"]}]});
            let fd = json!({"uid":3,"__type":case["kind"],"isArray":case["isArray"],"canBeNull":case["canBeNull"].as_bool().unwrap_or(true),"arrayMinLength":case["arrayMinLength"],"arrayMaxLength":case["arrayMaxLength"],"allowedRefs":case["allowedRefs"].as_str().unwrap_or("Any"),"allowedRefsEntityUid":1,"allowedRefTags":["allowed"]});
            assert_eq!(
                scene.field_error(&owner, &fd).as_deref(),
                case["expected"].as_str(),
                "{case}"
            );
        }
    }
    #[test]
    fn invalid_hidden_fields_remain_visible_and_array_errors_take_priority() {
        let project = Project::new();
        let ctx = egui::Context::default();
        let view = crate::viewport::Viewport::default();
        let hidden = std::collections::HashSet::new();
        let options = super::super::render_tests::options(
            &hidden,
            LevelKey {
                world: None,
                level: 0,
            },
        );
        let mut assets = crate::assets::Assets::default();
        let scene = Scene {
            ctx: &ctx,
            project: &project,
            view: &view,
            canvas: Rect::EVERYTHING,
            projection_origin: Rect::EVERYTHING.min.into(),
            options: &options,
            assets: &mut assets,
            entity_index: Default::default(),
            file_states: Default::default(),
        };
        let owner = json!({"fieldInstances":[{"defUid":1,"__value":null}]});
        let mut fd = json!({"uid":1,"identifier":"Target","__type":"EntityRef","editorDisplayMode":"Hidden","canBeNull":false});
        let row = scene.row(&owner, &fd, Color32::RED).unwrap();
        assert_eq!(row.error.as_deref(), Some("Value required"));
        assert_eq!(row.label, "Target");
        fd["__type"] = json!("Int");
        assert!(scene.field_error(&owner, &fd).is_none());
        fd["__type"] = json!("Array<EntityRef>");
        fd["isArray"] = json!(true);
        fd["arrayMinLength"] = json!(2);
        let owner = json!({"fieldInstances":[{"defUid":1,"__value":[{"entityIid":"missing"}]}]});
        assert_eq!(
            scene.field_error(&owner, &fd).as_deref(),
            Some("Array too short")
        );
        fd["arrayMinLength"] = json!(0);
        assert_eq!(
            scene.field_error(&owner, &fd).as_deref(),
            Some("Lost reference!")
        );
    }
    #[test]
    fn arrows_line_clamps_source_out_of_range_alpha_on_short_links() {
        let ctx = egui::Context::default();
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            let painter = ctx.layer_painter(egui::LayerId::background());
            for len in [0., 1., 15., 16., 16.1, 17., 31., 32., 33.] {
                render_link(
                    &painter,
                    Pos2::ZERO,
                    egui::pos2(len, 0.),
                    Color32::WHITE,
                    "ArrowsLine",
                    1.,
                    LinkEnding::None,
                );
            }
        });
    }
    #[test]
    fn field_display_respects_default_state_not_value_equality() {
        let fd = json!({"uid":1,"editorDisplayMode":"NameAndValue","editorAlwaysShow":false});
        let mut owner =
            json!({"fieldInstances":[{"defUid":1,"__value":5,"realEditorValues":[null]}]});
        assert!(!displayed(&owner, &fd, false));
        owner["fieldInstances"][0]["realEditorValues"] = json!([{"id":"V_Int","params":[5]}]);
        assert!(displayed(&owner, &fd, false));
    }
    #[test]
    fn numeric_prefix_and_boolean_value_labels_match_source() {
        assert_eq!(
            display_value(
                &json!(42),
                &json!({"__type":"Int","editorTextPrefix":"HP:","editorTextSuffix":"%"})
            ),
            "HP:42%"
        );
        assert_eq!(
            display_value(
                &json!(false),
                &json!({"__type":"Bool","editorDisplayMode":"ValueOnly","identifier":"locked"})
            ),
            "-locked"
        );
    }
}
