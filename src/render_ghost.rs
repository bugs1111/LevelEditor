use super::*;

pub(super) fn point_origin(entity: &Value, def: &Value, grid: f32) -> [f32; 2] {
    let pivot = pair(&entity["px"], [0.; 2]);
    let bounds = entities::entity_bounds(entity, def);
    std::array::from_fn(|axis| {
        let (resizable, pivot_key, center) = if axis == 0 {
            ("resizableX", "pivotX", bounds.center().x)
        } else {
            ("resizableY", "pivotY", bounds.center().y)
        };
        if def[resizable] == true {
            center
        } else {
            ((pivot[axis]
                - if number(def, pivot_key, 0.) == 1. {
                    1.
                } else {
                    0.
                })
                / grid)
                .trunc()
                * grid
                + grid * 0.5
        }
    })
}
impl Scene<'_> {
    pub(super) fn hovered_entity_cursor(
        &mut self,
        painter: &Painter,
        entity: &Value,
        transform: Transform,
        ui_scale: f32,
    ) {
        let Some(def) = values(&self.project.json["defs"]["entities"])
            .iter()
            .find(|def| def["uid"] == entity["defUid"])
        else {
            return;
        };
        let bounds = entities::entity_bounds(entity, def);
        let rect = self.screen(transform.rect(bounds));
        let scale = self.view.zoom * transform.scale;
        let bevel = 3. * scale;
        let outer = rect.expand(bevel);
        let points = match text(def, "renderMode", "Rectangle") {
            "Ellipse" | "Cross" => {
                let segments = if bounds.width() <= 32. && bounds.height() <= 32. {
                    24
                } else {
                    // Heaps uses ceil(radiusY * 3.14 / 2) for automatic segments.
                    ((bounds.height() * 0.5 + 3.) * 1.57).ceil().min(4096.) as usize
                };
                let radius = outer.size() * 0.5;
                (0..segments)
                    .map(|i| {
                        let angle = i as f32 * std::f32::consts::TAU / segments as f32;
                        outer.center() + egui::vec2(angle.cos(), angle.sin()) * radius
                    })
                    .collect()
            }
            _ => {
                let b = egui::vec2(bevel, 0.);
                let v = egui::vec2(0., bevel);
                vec![
                    outer.left_top() + b,
                    outer.right_top() - b,
                    outer.right_top() + v,
                    outer.right_bottom() - v,
                    outer.right_bottom() - b,
                    outer.left_bottom() + b,
                    outer.left_bottom() - v,
                    outer.left_top() + v,
                ]
            }
        };
        let mut path = egui::epaint::tessellator::Path::default();
        path.add_line_loop(&points);
        let mut outline = Mesh::default();
        // The source cursor uses a hard-edged world-pixel stroke.
        path.stroke_closed(0., &Stroke::new(scale, theme::ACCENT).into(), &mut outline);
        painter.add(egui::Shape::mesh(outline));

        let label = text(def, "identifier", "");
        let color = self.smart_color(entity, def);
        let pad = egui::vec2(3., 1.) * (ui_scale / self.ctx.pixels_per_point());
        let size = bitmap_font::measure_flow(self.ctx, label, ui_scale, f32::INFINITY) + pad * 2.;
        let pivot = pair(&entity["px"], [0.; 2]);
        let zoom = self.view.zoom;
        let offset =
            bounds.center_top() - Pos2::from(pivot) - egui::vec2(size.x * 0.5, size.y) / zoom;
        let min = self.point(transform.point(pivot))
            + egui::vec2(offset.x.trunc(), offset.y.trunc()) * zoom;
        painter.rect_filled(
            Rect::from_min_size(min, size),
            0.,
            color.lerp_to_gamma(Color32::BLACK, 0.5),
        );
        bitmap_font::paint(
            self.ctx,
            painter,
            self.assets,
            min + pad,
            label,
            color.lerp_to_gamma(Color32::WHITE, 0.5),
            ui_scale,
            f32::INFINITY,
            false,
        );
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn ghost_tiles(
        &mut self,
        painter: &Painter,
        layer: &Value,
        def: &Value,
        tiles: &[Value],
        transform: Transform,
        grid: f32,
        mode: GhostMode,
    ) {
        let Some(atlas) = layer["__tilesetDefUid"]
            .as_i64()
            .and_then(|uid| self.project.tileset(uid))
        else {
            return;
        };
        let Some(texture) = self.texture(atlas) else {
            return;
        };
        let tile_size = number(atlas, "tileGridSize", grid);
        let image_size = texture.size_vec2();
        let mut mesh = Mesh::with_texture(texture.id());
        for tile in tiles {
            let flips = tile["f"].as_i64().unwrap_or(0);
            let mut anchor = transform.point(pair(&tile["px"], [0.; 2]));
            let size = if mode == GhostMode::Brush {
                tile_size * transform.scale
            } else {
                tile_size
            };
            for (axis, coordinate) in anchor.iter_mut().enumerate() {
                let flipped = flips & (1 << axis) != 0;
                if mode == GhostMode::Brush {
                    let pivot = number(
                        def,
                        if axis == 0 {
                            "tilePivotX"
                        } else {
                            "tilePivotY"
                        },
                        0.,
                    );
                    let step = grid * (tile_size / grid).round().max(1.) * transform.scale;
                    *coordinate += if flipped {
                        step - (1. - pivot) * size
                    } else {
                        -pivot * size
                    };
                } else if flipped {
                    *coordinate += grid * transform.scale - size;
                }
            }
            let dst = self.screen([anchor[0], anchor[1], anchor[0] + size, anchor[1] + size]);
            if !dst.intersects(painter.clip_rect()) {
                continue;
            }
            let uv = tile_uv(
                pair(&tile["src"], [0.; 2]),
                [tile_size; 2],
                image_size.into(),
                flips,
            );
            mesh.add_rect_with_uv(
                dst,
                Rect::from_min_max(uv[0].into(), uv[1].into()),
                Color32::WHITE,
            );
        }
        painter.add(egui::Shape::mesh(mesh));
    }
    pub(super) fn ghost_entity_links(
        &self,
        painter: &Painter,
        entity: &Value,
        def: &Value,
        origin_transform: Transform,
        transform: Transform,
        grid: f32,
    ) {
        let origin = self.point(origin_transform.point(point_origin(entity, def, grid)));
        let color = lighten(self.smart_color(entity, def), 0.5);
        for field in values(&def["fieldDefs"]) {
            let mode = text(field, "editorDisplayMode", "");
            if !matches!(mode, "PointStar" | "PointPath" | "PointPathLoop") {
                continue;
            }
            let value = fields::field_value(entity, field);
            let items = if field["isArray"] == true {
                values(&value).iter().collect::<Vec<_>>()
            } else {
                vec![value.as_ref()]
            };
            for (index, point) in items.into_iter().enumerate() {
                if point.is_null() || (index > 0 && mode == "PointPath") {
                    continue;
                }
                let target = self.point(transform.point([
                    (number(point, "cx", 0.) + 0.5) * grid,
                    (number(point, "cy", 0.) + 0.5) * grid,
                ]));
                painter.line_segment([origin, target], Stroke::new(self.view.zoom, color));
            }
        }
    }
    #[allow(clippy::too_many_arguments)]
    pub(super) fn ghost_point(
        &self,
        painter: &Painter,
        entity: &Value,
        entity_def: &Value,
        field: &Value,
        point: &Value,
        entry: &Value,
        moved_owner: Option<&Value>,
        transform: Transform,
        grid: f32,
        mode: GhostMode,
        remap: impl Fn(&Value) -> Value,
    ) {
        let mapped = remap(&point["value"]);
        let position = |v: &Value| {
            self.point(transform.point([
                (number(v, "cx", 0.) + 0.5) * grid,
                (number(v, "cy", 0.) + 0.5) * grid,
            ]))
        };
        let target = position(&mapped);
        let color = self.smart_color(entity, entity_def);
        let radius = grid * transform.scale * self.view.zoom;
        painter.circle_stroke(target, radius * 0.5, Stroke::new(self.view.zoom, color));
        painter.circle_filled(target, radius * 0.3, color);
        if mode != GhostMode::Move {
            return;
        }
        let bright = lighten(color, 0.5);
        let stroke = Stroke::new(self.view.zoom, bright);
        let index = point["arrayIndex"].as_u64().unwrap_or(0) as usize;
        let display = text(field, "editorDisplayMode", "");
        if display == "PointStar" || index == 0 {
            let owner = moved_owner.unwrap_or(entity);
            painter.line_segment(
                [
                    target,
                    self.point(transform.point(point_origin(owner, entity_def, grid))),
                ],
                stroke,
            );
        }
        if display == "PointPath" {
            let value = fields::field_value(entity, field);
            let items = values(&value);
            for adjacent in [index.checked_sub(1), index.checked_add(1)]
                .into_iter()
                .flatten()
            {
                let Some(neighbor) = items.get(adjacent).filter(|v| !v.is_null()) else {
                    continue;
                };
                let selected = values(&entry["points"]).iter().any(|p| {
                    p["entityIid"] == entity["iid"]
                        && p["fieldDefUid"] == field["uid"]
                        && p["arrayIndex"].as_u64().unwrap_or(0) as usize == adjacent
                });
                let mut end = if selected {
                    position(&remap(neighbor))
                } else {
                    position(neighbor)
                };
                if selected {
                    // Original GenericLevelElementGroup uses pxParallaxX for
                    // selected adjacent points' Y component in PointPath links.
                    end.y += (transform.origin[0] - transform.origin[1]) * self.view.zoom;
                }
                painter.line_segment([target, end], stroke);
            }
        }
    }
}
