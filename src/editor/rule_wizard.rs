use super::*;
use std::collections::BTreeMap;
#[derive(Clone)]
struct Wizard {
    layer: i64,
    group: Option<i64>,
    name: String,
    main: i64,
    other: i64,
    break_on_match: bool,
    selected: usize,
    fragments: BTreeMap<usize, Vec<i64>>,
}
#[derive(Clone)]
struct Fragment {
    name: &'static str,
    x: usize,
    y: usize,
    pattern: [i64; 9],
}
fn flip(pattern: [i64; 9], x: bool, y: bool) -> [i64; 9] {
    std::array::from_fn(|i| {
        pattern[(if x { 2 - i % 3 } else { i % 3 }) + 3 * (if y { 2 - i / 3 } else { i / 3 })]
    })
}
fn fragments() -> Vec<Fragment> {
    let mut out = Vec::new();
    let mut add = |name, x, y, rows: [&str; 3], fx, fy| {
        let mut p = [0; 9];
        for (i, c) in rows.join("").chars().enumerate() {
            p[i] = match c {
                'o' => 1,
                'x' => -1,
                _ => 0,
            };
        }
        out.push(Fragment {
            name,
            x,
            y,
            pattern: flip(p, fx, fy),
        });
    };
    add("Single", 3, 1, ["-x-", "xox", "-x-"], false, false);
    add("Cross", 3, 2, ["xox", "ooo", "xox"], false, false);
    for (name, x, y, fx, fy) in [
        ("Turn NW", 2, 3, false, false),
        ("Turn NE", 3, 3, true, false),
        ("Turn SE", 3, 4, true, true),
        ("Turn SW", 2, 4, false, true),
    ] {
        add(name, x, y, ["-x-", "xoo", "-ox"], fx, fy);
    }
    add("Diagonal SW NE", 10, 2, ["xoo", "ooo", "oox"], false, false);
    add("Diagonal NW SE", 10, 3, ["oox", "ooo", "xoo"], false, false);
    for (name, x, y, rows, fx, fy) in [
        ("Corner NW to N", 6, 2, ["-ox", "xoo", "-oo"], false, false),
        ("Corner NW to W", 6, 0, ["-x-", "ooo", "xoo"], false, false),
        (
            "Corner NW to NW",
            10,
            0,
            ["xox", "ooo", "xoo"],
            false,
            false,
        ),
        ("Corner NE to N", 7, 2, ["-ox", "xoo", "-oo"], true, false),
        ("Corner NE to E", 7, 0, ["-x-", "ooo", "xoo"], true, false),
        ("Corner NE to NE", 11, 0, ["xox", "ooo", "xoo"], true, false),
        ("Corner SE to E", 7, 1, ["-x-", "ooo", "xoo"], true, true),
        ("Corner SE to S", 7, 3, ["-ox", "xoo", "-oo"], true, true),
        ("Corner SE to SE", 11, 1, ["xox", "ooo", "xoo"], true, true),
        ("Corner SW to S", 6, 3, ["-ox", "xoo", "-oo"], false, true),
        ("Corner SW to W", 6, 1, ["-x-", "ooo", "xoo"], false, true),
        ("Corner SW to SW", 10, 1, ["xox", "ooo", "xoo"], false, true),
        ("T cross N", 8, 3, ["xox", "ooo", "-x-"], false, false),
        ("T cross E", 8, 2, ["xo-", "oox", "xo-"], true, false),
        ("T cross S", 9, 2, ["xox", "ooo", "-x-"], false, true),
        ("T cross W", 9, 3, ["xo-", "oox", "xo-"], false, false),
        ("T wall W", 8, 0, ["xoo", "ooo", "xoo"], false, false),
        ("T wall E", 9, 0, ["xoo", "ooo", "xoo"], true, false),
        ("T wall S", 8, 1, ["xox", "ooo", "ooo"], false, true),
        ("T wall N", 9, 1, ["xox", "ooo", "ooo"], false, false),
        ("Horizontal W", 3, 0, ["-x-", "xo-", "-x-"], false, false),
        ("Horizontal E", 5, 0, ["-x-", "xo-", "-x-"], true, false),
        (
            "Horizontal middle",
            4,
            0,
            ["-x-", "-o-", "-x-"],
            false,
            false,
        ),
        ("Vertical N", 4, 1, ["-x-", "xox", "---"], false, false),
        ("Vertical S", 4, 3, ["-x-", "xox", "---"], false, true),
        ("Vertical middle", 4, 2, ["---", "xox", "---"], false, false),
        ("Outer corner NW", 0, 0, ["-x-", "xo-", "---"], false, false),
        ("Outer corner NE", 2, 0, ["-x-", "xo-", "---"], true, false),
        ("Outer corner SE", 2, 2, ["-x-", "xo-", "---"], true, true),
        ("Outer corner SW", 0, 2, ["-x-", "xo-", "---"], false, true),
        ("Wall N", 1, 0, ["-x-", "-o-", "---"], false, false),
        ("Wall E", 2, 1, ["---", "xo-", "---"], true, false),
        ("Wall S", 1, 2, ["-x-", "-o-", "---"], false, true),
        ("Wall W", 0, 1, ["---", "xo-", "---"], false, false),
        ("Inner corner NW", 0, 3, ["xo-", "oo-", "---"], false, false),
        ("Inner corner NE", 1, 3, ["xo-", "oo-", "---"], true, false),
        ("Inner corner SE", 1, 4, ["xo-", "oo-", "---"], true, true),
        ("Inner corner SW", 0, 4, ["xo-", "oo-", "---"], false, true),
        ("Full", 1, 1, ["---", "-o-", "---"], false, false),
    ] {
        add(name, x, y, rows, fx, fy);
    }
    out
}
fn alternative(
    all: &[Fragment],
    assigned: &BTreeMap<usize, Vec<i64>>,
    index: usize,
) -> Option<(usize, bool, bool)> {
    for (fx, fy) in [(true, false), (false, true), (true, true)] {
        let flipped = flip(all[index].pattern, fx, fy);
        if let Some((i, _)) = all
            .iter()
            .enumerate()
            .find(|(i, f)| *i != index && f.pattern == flipped && assigned.contains_key(i))
        {
            return Some((i, fx, fy));
        }
    }
    None
}
fn rules(w: &Wizard) -> Vec<Value> {
    let all = fragments();
    let mut result = Vec::new();
    for (i, fragment) in all.iter().enumerate() {
        let Some(ids) = w.fragments.get(&i).filter(|v| !v.is_empty()) else {
            continue;
        };
        let mut fx = false;
        let mut fy = false;
        for (j, _) in all.iter().enumerate() {
            if !w.fragments.contains_key(&j)
                && let Some((source, x, y)) = alternative(&all, &w.fragments, j)
                && source == i
            {
                fx |= x;
                fy |= y;
            }
        }
        let p = fragment.pattern.map(|v| match v {
            1 => w.main,
            -1 => {
                if w.other > 0 {
                    w.other
                } else {
                    -w.main
                }
            }
            _ => 0,
        });
        let (size, pattern) = if p.iter().enumerate().all(|(i, v)| i == 4 || *v == 0) {
            (1, vec![p[4]])
        } else {
            (3, p.to_vec())
        };
        result.push(json!({"size":size,"pattern":pattern,"tileRectsIds":ids.iter().map(|id|vec![*id]).collect::<Vec<_>>(),"tileMode":"Single","flipX":fx,"flipY":fy,"breakOnMatch":w.break_on_match,"outOfBoundsValue":w.main}));
    }
    result
}
impl EditorApp {
    pub(super) fn begin_rule_wizard(&self, ctx: &egui::Context, layer: i64, group: Option<i64>) {
        let Some(def) = self.project.as_ref().and_then(|p| p.layer_def(layer)) else {
            return;
        };
        let mut w = Wizard {
            layer,
            group,
            name: "Auto rules".into(),
            main: 0,
            other: 0,
            break_on_match: true,
            selected: 0,
            fragments: BTreeMap::new(),
        };
        let source = def["autoSourceLayerDefUid"]
            .as_i64()
            .and_then(|uid| self.project.as_ref()?.layer_def(uid))
            .unwrap_or(def);
        if source["intGridValues"]
            .as_array()
            .is_some_and(|v| v.len() == 1)
            && let Some(v) = source["intGridValues"][0]["value"].as_i64()
        {
            w.main = v;
            w.name = format!(
                "Rules for {}",
                text(&source["intGridValues"][0], "identifier", &v.to_string())
            );
        }
        if let Some(group) = group.and_then(|uid| {
            def["autoRuleGroups"]
                .as_array()?
                .iter()
                .find(|g| g["uid"] == uid)
        }) {
            w.name = text(group, "name", "Auto rules").into();
            let source_rules = group["rules"].as_array().cloned().unwrap_or_default();
            w.main = source_rules
                .iter()
                .find_map(|r| {
                    let n = r["size"].as_u64()? as usize;
                    let v = r["pattern"][n * n / 2].as_i64()?;
                    (v > 0).then_some(v)
                })
                .unwrap_or(w.main);
            w.other = source_rules
                .iter()
                .flat_map(|r| r["pattern"].as_array().into_iter().flatten())
                .filter_map(Value::as_i64)
                .map(i64::abs)
                .find(|v| *v != w.main && *v > 0 && *v < 1000001)
                .unwrap_or(0);
            w.break_on_match = source_rules
                .first()
                .and_then(|r| r["breakOnMatch"].as_bool())
                .unwrap_or(true);
            let all = fragments();
            for r in source_rules {
                let raw = r["pattern"].as_array().cloned().unwrap_or_default();
                let mut p = [0; 9];
                if raw.len() == 1 {
                    p[4] = 1
                } else if raw.len() == 9 {
                    for (i, v) in raw.iter().enumerate() {
                        let v = v.as_i64().unwrap_or(0);
                        p[i] = if v == w.main {
                            1
                        } else if v == w.other && v > 0 || v == -w.main {
                            -1
                        } else {
                            0
                        };
                    }
                } else {
                    continue;
                }
                if let Some((i, _)) = all.iter().enumerate().find(|(_, f)| f.pattern == p) {
                    let ids = r["tileRectsIds"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(|r| r[0].as_i64())
                        .collect::<Vec<_>>();
                    if !ids.is_empty() {
                        w.fragments.insert(i, ids);
                    }
                }
            }
        }
        ctx.data_mut(|d| d.insert_temp(egui::Id::new("source-rule-wizard"), w));
    }
    pub(super) fn rule_wizard(&mut self, ctx: &egui::Context) {
        let id = egui::Id::new("source-rule-wizard");
        if let Some(state) = ctx.data_mut(|d| d.get_temp::<Wizard>(id)) {
            self.with_local_atlas_state(ctx, id.with((state.layer, state.group)), |app| {
                app.rule_wizard_inner(ctx)
            });
        }
    }
    fn rule_wizard_inner(&mut self, ctx: &egui::Context) {
        let id = egui::Id::new("source-rule-wizard");
        let Some(mut w) = ctx.data_mut(|d| d.get_temp::<Wizard>(id)) else {
            return;
        };
        let Some(def) = self
            .project
            .as_ref()
            .and_then(|p| p.layer_def(w.layer))
            .cloned()
        else {
            ctx.data_mut(|d| d.remove::<Wizard>(id));
            return;
        };
        let source = def["autoSourceLayerDefUid"]
            .as_i64()
            .and_then(|uid| self.project.as_ref()?.layer_def(uid))
            .unwrap_or(&def)
            .clone();
        let atlas = def["tilesetDefUid"].as_i64().unwrap_or(-1);
        let all = fragments();
        let mut open = true;
        let mut apply = false;
        egui::Window::new("Auto-layer rules assistant").id(id).open(&mut open).default_width(1000.).resizable(false).show(ctx,|ui|{
            ui.horizontal_top(|ui|{
                ui.allocate_ui_with_layout(egui::vec2(560.,600.),egui::Layout::top_down(egui::Align::Min),|ui|{
                    ui.set_width(560.);ui.set_max_width(560.);
            ui.label("Select a wall fragment, then assign its tiles. Right click a fragment to clear it. Mirrored fragments are inferred automatically.");
            ui.horizontal(|ui|{ui.label("Group name");ui.text_edit_singleline(&mut w.name);let mut stack=!w.break_on_match;if ui.checkbox(&mut stack,"Allow rules to stack").changed(){w.break_on_match = !stack;}});
            let old_main=w.main;
            ui.horizontal(|ui|{for(other,label)in[(false,"Main value"),(true,"Other value")]{ui.label(label);let value=if other{&mut w.other}else{&mut w.main};egui::ComboBox::from_id_salt((id,other)).selected_text(if *value==0{if other{"Anything except main".into()}else{"Pick a value".into()}}else{value.to_string()}).show_ui(ui,|ui|{if other{ui.selectable_value(value,0,"Anything except main");}for v in source["intGridValues"].as_array().into_iter().flatten(){if let Some(n)=v["value"].as_i64(){ui.selectable_value(value,n,format!("{} {}",n,text(v,"identifier","")));}}});}});
            if w.main!=old_main{if w.main==w.other{w.other=0;}
if w.group.is_none(){w.name=source["intGridValues"].as_array().and_then(|a|a.iter().find(|v|v["value"]==w.main)).and_then(|v|v["identifier"].as_str()).filter(|s|!s.is_empty()).map(str::to_owned).unwrap_or_else(||format!("Rules for #{}",w.main));}}
            let (grid,_)=ui.allocate_exact_size(egui::vec2(12.*46.,5.*46.),Sense::hover());
            for(i,f)in all.iter().enumerate(){let rect=Rect::from_min_size(grid.min+egui::vec2(f.x as f32*46.,f.y as f32*46.),Vec2::splat(44.));let response=ui.interact(rect,id.with(i),Sense::click());ui.painter().rect_filled(rect,2.,if i==w.selected{theme::ACCENT}else{theme::DARK});
                let inferred=alternative(&all,&w.fragments,i);let ids=w.fragments.get(&i).or_else(||inferred.and_then(|(j,_,_)|w.fragments.get(&j)));
                if let Some(tid)=ids.and_then(|v|v.first())&&let Some(ts)=self.project.as_ref().and_then(|p|p.tileset(atlas)).cloned(){
                    let texture=if ts["embedAtlas"].is_null(){ts["relPath"].as_str().and_then(|rel|self.assets.texture(ctx,&canvas::asset_path(self.project.as_ref().unwrap(),rel)))}else{self.assets.embedded(ctx)};
                    if let Some(texture)=texture{let size=texture.size_vec2();let grid=number(&ts,"tileGridSize",16.);let cols=number(&ts,"__cWid",1.)as i64;let stride=grid+number(&ts,"spacing",0.);let x=number(&ts,"padding",0.)+(*tid%cols.max(1))as f32*stride;let y=number(&ts,"padding",0.)+(*tid/cols.max(1))as f32*stride;let mut uv=Rect::from_min_max(egui::pos2(x/size.x,y/size.y),egui::pos2((x+grid)/size.x,(y+grid)/size.y));if !w.fragments.contains_key(&i)&&let Some((_,fx,fy))=inferred{if fx{std::mem::swap(&mut uv.min.x,&mut uv.max.x);}
if fy{std::mem::swap(&mut uv.min.y,&mut uv.max.y);}}ui.painter().image(texture.id(),rect,uv,Color32::WHITE.gamma_multiply(if w.fragments.contains_key(&i){1.}else{0.4}));}
                }
                let(x,y)=FRAGMENT_ICONS[i];egui::Image::new(egui::include_image!("../../assets/icons/wallFragments.png")).uv(Rect::from_min_max(egui::pos2(x as f32/176.,y as f32/208.),egui::pos2((x+16)as f32/176.,(y+16)as f32/208.))).tint(Color32::WHITE.gamma_multiply(if ids.is_some(){0.2}else{1.})).texture_options(egui::TextureOptions::NEAREST).paint_at(ui,rect);
                if response.clicked(){w.selected=i;let ids=w.fragments.get(&i).map(|v|v.iter().map(|id|*id as i32).collect::<Vec<_>>()).unwrap_or_default();if ids.is_empty(){self.atlas_selection=Some((atlas,vec![]));}else{self.set_atlas_selection(atlas,ids);}}

if response.secondary_clicked()||response.middle_clicked(){w.fragments.remove(&i);}response.on_hover_text(f.name);
            }
            ui.label(format!("Selected: {}",all[w.selected].name));
                });
                ui.allocate_ui_with_layout(egui::vec2(380.,600.),egui::Layout::top_down(egui::Align::Min),|ui|{
                    ui.set_width(380.);ui.set_max_width(380.);
            if atlas>=0{let before=self.selected_tile_ids_for(atlas);self.atlas_picker(ctx,ui,atlas);let after=self.selected_tile_ids_for(atlas);if before!=after&&!after.is_empty(){w.fragments.insert(w.selected,after.into_iter().map(i64::from).collect());}ui.horizontal(|ui|{if source_controls::button(ui,"Assign selected tiles").clicked(){let ids=self.selected_tile_ids_for(atlas);if !ids.is_empty(){w.fragments.insert(w.selected,ids.into_iter().map(i64::from).collect());}}
if source_controls::button(ui,"Clear fragment").clicked(){w.fragments.remove(&w.selected);}});}else{ui.colored_label(theme::ACCENT,"Assign an output tileset in the layer settings first.");}
                });
            });
            ui.horizontal(|ui|{if source_controls::add_enabled_button(ui,w.main>0&&w.main!=w.other&&!w.fragments.is_empty()&&!w.name.trim().is_empty(),egui::Button::new(if w.group.is_some(){"Replace group rules"}else{"Create rules"})).clicked(){apply=true;}ui.label(format!("{} rules",rules(&w).len()));});
        });
        if apply {
            let patches = rules(&w);
            if let Some(gid) = self.edit("Wizard rules generated", |p| {
                p.apply_auto_rule_wizard(w.layer, w.group, &w.name, &patches)
            }) {
                self.rule_group_uid = Some(gid);
                self.rule_uid = None;
                open = false;
            }
        }
        if open {
            ctx.data_mut(|d| d.insert_temp(id, w));
        } else {
            ctx.data_mut(|d| d.remove::<Wizard>(id));
        }
    }
}
#[derive(Clone)]
struct GroupRemap {
    layer: i64,
    group: i64,
    name: String,
    remaps: BTreeMap<i64, i64>,
    offset: [i32; 2],
    first_tile: i32,
}
impl EditorApp {
    pub(super) fn begin_group_remap(&self, ctx: &egui::Context, layer: i64, group: i64) {
        let Some(p) = self.project.as_ref() else {
            return;
        };
        let Ok(data) = p.copy_auto_rule(layer, group, None) else {
            return;
        };
        let mut remaps = BTreeMap::new();
        let mut first = None;
        for rule in data.value["rules"].as_array().into_iter().flatten() {
            for n in rule["pattern"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(Value::as_i64)
            {
                let n = n.abs();
                if n > 0 && n != 1000001 {
                    remaps.insert(n, n);
                }
            }
            for n in rule["tileRectsIds"]
                .as_array()
                .into_iter()
                .flatten()
                .flat_map(|r| r.as_array().into_iter().flatten())
                .filter_map(Value::as_i64)
            {
                first = Some(first.map_or(n, |old: i64| old.min(n)));
            }
        }
        let state = GroupRemap {
            layer,
            group,
            name: format!("{} copy", text(&data.value, "name", "Group")),
            remaps,
            offset: [0, 0],
            first_tile: first.unwrap_or(0) as i32,
        };
        ctx.data_mut(|d| d.insert_temp(egui::Id::new("source-group-remap"), state));
    }
    pub(super) fn group_remap_dialog(&mut self, ctx: &egui::Context) {
        let id = egui::Id::new("source-group-remap");
        if let Some(state) = ctx.data_mut(|d| d.get_temp::<GroupRemap>(id)) {
            self.with_local_atlas_state(ctx, id.with((state.layer, state.group)), |app| {
                app.group_remap_dialog_inner(ctx)
            });
        }
    }
    fn group_remap_dialog_inner(&mut self, ctx: &egui::Context) {
        let id = egui::Id::new("source-group-remap");
        let Some(mut state) = ctx.data_mut(|d| d.get_temp::<GroupRemap>(id)) else {
            return;
        };
        let Some(def) = self
            .project
            .as_ref()
            .and_then(|p| p.layer_def(state.layer))
            .cloned()
        else {
            ctx.data_mut(|d| d.remove::<GroupRemap>(id));
            return;
        };
        let atlas = def["tilesetDefUid"].as_i64().unwrap_or(-1);
        let source = def["autoSourceLayerDefUid"]
            .as_i64()
            .and_then(|uid| self.project.as_ref()?.layer_def(uid))
            .unwrap_or(&def)
            .clone();
        let mut open = true;
        let mut apply = false;
        egui::Window::new("Duplicate and remap rule group")
            .id(id)
            .open(&mut open)
            .default_width(760.)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("New group name");
                    ui.text_edit_singleline(&mut state.name);
                });
                ui.label("Remap IntGrid values");
                for (old, new) in &mut state.remaps {
                    ui.horizontal(|ui| {
                        ui.label(format!("{old} →"));
                        egui::ComboBox::from_id_salt((id, old))
                            .selected_text(if old == new {
                                "No change".into()
                            } else {
                                new.to_string()
                            })
                            .show_ui(ui, |ui| {
                                ui.selectable_value(new, 0, "Empty");
                                for value in
                                    source["intGridValues"].as_array().into_iter().flatten()
                                {
                                    if let Some(n) = value["value"].as_i64() {
                                        ui.selectable_value(
                                            new,
                                            n,
                                            format!("{} {}", n, text(value, "identifier", "")),
                                        );
                                    }
                                }
                                for group in source["intGridValuesGroups"]
                                    .as_array()
                                    .into_iter()
                                    .flatten()
                                {
                                    if let Some(uid) = group["uid"].as_i64() {
                                        ui.selectable_value(
                                            new,
                                            (uid + 1) * 1000,
                                            format!("Group {}", text(group, "identifier", "")),
                                        );
                                    }
                                }
                            });
                    });
                }
                ui.separator();
                ui.label("Transpose tiles");
                ui.horizontal(|ui| {
                    ui.label("Offset X");
                    ui.add(egui::DragValue::new(&mut state.offset[0]));
                    ui.label("Y");
                    ui.add(egui::DragValue::new(&mut state.offset[1]));
                });
                if atlas >= 0 {
                    let before = self.tile_id;
                    self.atlas_picker(ctx, ui, atlas);
                    if before != self.tile_id
                        && let Some(ts) = self.project.as_ref().and_then(|p| p.tileset(atlas))
                    {
                        let cols = number(ts, "__cWid", 1.) as i32;
                        state.offset = [
                            self.tile_id % cols.max(1) - state.first_tile % cols.max(1),
                            self.tile_id / cols.max(1) - state.first_tile / cols.max(1),
                        ];
                    }
                }
                let mut preview = self.project.as_ref().unwrap().clone();
                let validation = preview.duplicate_rule_group_remapped(
                    state.layer,
                    state.group,
                    &state.name,
                    &state.remaps,
                    state.offset,
                );
                if let Err(error) = &validation {
                    ui.colored_label(theme::ACCENT, error.to_string());
                }
                if source_controls::add_enabled_button(
                    ui,
                    validation.is_ok(),
                    egui::Button::new("Confirm duplicate and remap"),
                )
                .clicked()
                {
                    apply = true;
                }
            });
        if apply
            && let Some(group) = self.edit("Rule group duplicated and remapped", |p| {
                p.duplicate_rule_group_remapped(
                    state.layer,
                    state.group,
                    &state.name,
                    &state.remaps,
                    state.offset,
                )
            })
        {
            self.rule_group_uid = Some(group);
            self.rule_uid = None;
            open = false;
        }
        if open {
            ctx.data_mut(|d| d.insert_temp(id, state));
        } else {
            ctx.data_mut(|d| d.remove::<GroupRemap>(id));
        }
    }
}

const FRAGMENT_ICONS: [(u16, u16); 47] = [
    (32, 48),
    (32, 64),
    (48, 0),
    (64, 0),
    (64, 16),
    (48, 16),
    (64, 32),
    (64, 48),
    (80, 32),
    (80, 0),
    (144, 0),
    (96, 32),
    (96, 0),
    (160, 0),
    (96, 16),
    (96, 48),
    (160, 16),
    (80, 48),
    (80, 16),
    (144, 16),
    (112, 48),
    (112, 32),
    (128, 32),
    (128, 48),
    (112, 0),
    (128, 0),
    (112, 16),
    (128, 16),
    (64, 64),
    (96, 64),
    (80, 64),
    (48, 32),
    (48, 64),
    (48, 48),
    (0, 0),
    (32, 0),
    (32, 32),
    (0, 32),
    (16, 0),
    (32, 16),
    (16, 32),
    (0, 16),
    (0, 48),
    (16, 48),
    (16, 64),
    (0, 64),
    (16, 16),
];

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn source_fragment_order_patterns_and_mirrored_rules() {
        let f = fragments();
        assert_eq!(f.len(), 47);
        assert_eq!(f[0].name, "Single");
        assert_eq!(f[46].name, "Full");
        let mut w = Wizard {
            layer: 1,
            group: None,
            name: "Walls".into(),
            main: 2,
            other: 0,
            break_on_match: true,
            selected: 0,
            fragments: BTreeMap::new(),
        };
        w.fragments.insert(42, vec![10, 11]);
        let r = rules(&w);
        assert_eq!(r[0]["pattern"], json!([-2, 2, 0, 2, 2, 0, 0, 0, 0]));
        assert_eq!(r[0]["flipX"], true);
        assert_eq!(r[0]["flipY"], true);
        assert_eq!(r[0]["tileRectsIds"], json!([[10], [11]]));
        w.fragments.clear();
        w.fragments.insert(46, vec![0]);
        assert_eq!(rules(&w)[0]["pattern"], json!([2]));
        assert_eq!(rules(&w)[0]["size"], 1);
    }
}
