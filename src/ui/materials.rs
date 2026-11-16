//! Materials section: pick a model part, choose what it's made of, add your own texture maps,
//! special kinds (glass, water, wet, fur, grass, glow), presets, and an optional node editor.

use crate::app::App;
use crate::effects::{MatPreset, ModelLook, NodeKind};
use crate::material::{builtin_material_presets, MatGraph, MatNodeKind, MaterialFx, Special};
use crate::ui::theme;
use egui::{pos2, vec2, Color32, CornerRadius, Pos2, Rect, RichText, Sense, Stroke};

const MN_W: f32 = 170.0;
const MN_HEAD: f32 = 24.0;
const MN_ROW: f32 = 20.0;

fn pick_image() -> Option<String> {
    rfd::FileDialog::new()
        .add_filter("Pictures", &["png", "jpg", "jpeg", "bmp", "tga", "dds"])
        .pick_file()
        .map(|p| p.display().to_string())
}

fn short(p: &str) -> String {
    std::path::Path::new(p).file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| p.to_string())
}

impl App {
    fn selected_material_name(&self) -> Option<String> {
        let (mi, mat) = self.mat_sel?;
        self.scene.models.get(mi)?.data.materials.get(mat).map(|m| m.name.clone())
    }

    fn auto_guess(&self, name: &str) -> MatPreset {
        let auto = self.graph.fx_of(NodeKind::RayLighting).map(|n| n.b("Auto materials")).unwrap_or(true);
        self.graph.materials.get(name).copied().unwrap_or(if auto { MatPreset::guess(name) } else { MatPreset::Cloth })
    }

    pub fn materials_list(&mut self, ui: &mut egui::Ui) {
        theme::section(ui, "Model looks");
        let names: Vec<String> = self.scene.models.iter().filter(|m| !m.is_accessory()).map(|m| m.name().to_string()).collect();
        for name in &names {
            ui.horizontal(|ui| {
                ui.label(RichText::new(name).strong());
                let cur = self.graph.model_looks.get(name).copied();
                egui::ComboBox::from_id_salt(("look", name)).width(170.0).selected_text(cur.map(|l| l.label()).unwrap_or("Same as scene")).show_ui(ui, |ui| {
                    if ui.selectable_label(cur.is_none(), "Same as scene").clicked() {
                        self.graph.model_looks.remove(name);
                    }
                    for l in ModelLook::ALL {
                        if ui.selectable_label(cur == Some(l), l.label()).clicked() {
                            self.graph.model_looks.insert(name.clone(), l);
                        }
                    }
                });
            });
        }
        ui.label(RichText::new("Materials matter most with the Realistic look; special kinds (glass, water, fur…) work in every look.").size(10.5).color(theme::TEXT_DIM));
        theme::section(ui, "Parts (materials)");
        let models: Vec<(usize, String, Vec<String>)> = self
            .scene
            .models
            .iter()
            .enumerate()
            .map(|(i, m)| (i, m.name().to_string(), m.data.materials.iter().map(|x| x.name.clone()).collect()))
            .collect();
        if models.is_empty() {
            ui.label(RichText::new("Load a model to edit its materials.").color(theme::TEXT_DIM));
        }
        for (mi, mname, mats) in models {
            egui::CollapsingHeader::new(RichText::new(&mname).strong()).id_salt(("mlist", mi)).default_open(true).show(ui, |ui| {
                for (k, name) in mats.iter().enumerate() {
                    let sel = self.mat_sel == Some((mi, k));
                    let kind = match self.graph.mat_fx.get(name) {
                        Some(f) => {
                            let s = if f.special != Special::None { f.special.label() } else { f.base.label() };
                            format!("{s}{}", if f.graph.is_some() { " · nodes" } else { "" })
                        }
                        None => format!("Auto · {}", self.auto_guess(name).label()),
                    };
                    let resp = ui.selectable_label(sel, RichText::new(format!("{name}   ")).strong());
                    let r = resp.rect;
                    ui.painter().text(pos2(r.right() + 4.0, r.center().y), egui::Align2::LEFT_CENTER, kind, egui::FontId::proportional(10.5), theme::TEXT_DIM);
                    if resp.clicked() {
                        self.mat_sel = Some((mi, k));
                        self.mnodes.selected = None;
                    }
                }
            });
        }
    }

    pub fn material_editor(&mut self, ui: &mut egui::Ui) {
        let Some(name) = self.selected_material_name() else {
            theme::section(ui, "Material");
            ui.label(RichText::new("Pick a part on the left to edit what it's made of.").color(theme::TEXT_DIM));
            return;
        };
        theme::section(ui, "Material");
        ui.label(RichText::new(&name).strong().size(15.0).color(theme::ACCENT));
        let guess = self.auto_guess(&name);
        if !self.graph.mat_fx.contains_key(&name) {
            ui.label(RichText::new(format!("Automatic: {}", guess.label())).color(theme::TEXT_DIM));
            if ui.button("Customise this material").clicked() {
                self.graph.mat_fx.insert(name.clone(), MaterialFx::from_preset(guess, 0.55));
            }
            self.material_presets_menu(ui, &name);
            return;
        }
        self.material_presets_menu(ui, &name);
        let Some(mut fx) = self.graph.mat_fx.get(&name).cloned() else { return };
        let before = fx.clone();
        theme::card(ui, |ui| {
            egui::ComboBox::from_label("Made of").selected_text(fx.base.label()).show_ui(ui, |ui| {
                for p in MatPreset::ALL {
                    if ui.selectable_label(fx.base == p, p.label()).clicked() && fx.base != p {
                        let keep = fx.clone();
                        fx = MaterialFx { normal_map: keep.normal_map, height_map: keep.height_map, metal_map: keep.metal_map, rough_map: keep.rough_map, graph: keep.graph, special: keep.special, ..MaterialFx::from_preset(p, 0.55) };
                    }
                }
            });
            egui::ComboBox::from_label("Special kind").selected_text(fx.special.label()).show_ui(ui, |ui| {
                for s in Special::ALL {
                    ui.selectable_value(&mut fx.special, s, s.label());
                }
            });
            ui.horizontal(|ui| {
                ui.color_edit_button_rgb(&mut fx.tint);
                ui.label("Colour tint");
            });
            ui.add(egui::Slider::new(&mut fx.rough, 0.02..=1.0).text("Roughness"));
            ui.add(egui::Slider::new(&mut fx.metal, 0.0..=1.0).text("Metallic"));
            ui.add(egui::Slider::new(&mut fx.spec, 0.0..=2.0).text("Shine"));
            ui.add(egui::Slider::new(&mut fx.sss, 0.0..=1.0).text("Skin scattering"));
        });

        theme::section(ui, "Your texture maps");
        theme::card(ui, |ui| {
            let mut row = |ui: &mut egui::Ui, label: &str, tip: &str, slot: &mut Option<String>| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(label).strong()).on_hover_text(tip);
                    let text = slot.as_deref().map(short).unwrap_or_else(|| "none".into());
                    ui.label(RichText::new(text).size(11.0).color(if slot.is_some() { Color32::WHITE } else { theme::TEXT_DIM }));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if slot.is_some() && ui.small_button("×").clicked() {
                            *slot = None;
                        }
                        if ui.small_button("Pick…").clicked() {
                            if let Some(p) = pick_image() {
                                *slot = Some(p);
                            }
                        }
                    });
                });
            };
            row(ui, "Normal", "Blue-ish normal map: fine bumps and detail", &mut fx.normal_map);
            if fx.normal_map.is_some() {
                ui.add(egui::Slider::new(&mut fx.normal_strength, 0.0..=3.0).text("Bump strength"));
            }
            row(ui, "Height", "Grey height / displacement map: carved, deep surfaces", &mut fx.height_map);
            if fx.height_map.is_some() {
                ui.add(egui::Slider::new(&mut fx.height_scale, 0.0..=0.15).text("Depth"));
            }
            row(ui, "Metallic", "White = metal, black = not metal", &mut fx.metal_map);
            row(ui, "Roughness", "White = rough, black = glossy", &mut fx.rough_map);
            ui.add(egui::Slider::new(&mut fx.uv_scale, 0.1..=20.0).logarithmic(true).text("Map tiling"));
            ui.label(RichText::new("Maps use the model's own texture layout (UVs).").size(10.5).color(theme::TEXT_DIM));
        });

        match fx.special {
            Special::None => {}
            Special::Glass => {
                theme::section(ui, "Glass");
                theme::card(ui, |ui| {
                    ui.add(egui::Slider::new(&mut fx.glass_opacity, 0.0..=1.0).text("Opacity"));
                });
            }
            Special::Water => {
                theme::section(ui, "Water");
                theme::card(ui, |ui| {
                    ui.add(egui::Slider::new(&mut fx.ripple_scale, 0.1..=5.0).text("Ripple size"));
                    ui.add(egui::Slider::new(&mut fx.ripple_speed, 0.0..=4.0).text("Ripple speed"));
                });
            }
            Special::Wet => {
                theme::section(ui, "Wet + rain");
                theme::card(ui, |ui| {
                    ui.add(egui::Slider::new(&mut fx.wet, 0.0..=1.0).text("Wetness"));
                    ui.add(egui::Slider::new(&mut fx.rain, 0.0..=1.0).text("Rain drops"));
                    ui.add(egui::Slider::new(&mut fx.ripple_scale, 0.1..=5.0).text("Drop size"));
                    ui.add(egui::Slider::new(&mut fx.ripple_speed, 0.0..=4.0).text("Drop speed"));
                });
            }
            Special::Fur | Special::Grass => {
                theme::section(ui, if fx.special == Special::Fur { "Fur" } else { "Grass" });
                theme::card(ui, |ui| {
                    ui.add(egui::Slider::new(&mut fx.fur_length, 0.02..=3.0).text("Length"));
                    ui.add(egui::Slider::new(&mut fx.fur_density, 10.0..=800.0).logarithmic(true).text("Density"));
                    ui.add(egui::Slider::new(&mut fx.fur_layers, 4..=48).text("Layers (quality)"));
                    ui.add(egui::Slider::new(&mut fx.fur_droop, 0.0..=1.0).text("Droop"));
                });
            }
            Special::Glow => {}
        }
        theme::section(ui, "Glow / luminous");
        theme::card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.color_edit_button_rgb(&mut fx.emissive_color);
                ui.add(egui::Slider::new(&mut fx.emissive, 0.0..=10.0).text("Glow"));
            });
            ui.checkbox(&mut fx.emit_light, "Light up the surroundings (becomes a light source)");
            if fx.emit_light {
                ui.add(egui::Slider::new(&mut fx.light_range, 2.0..=150.0).text("Light reach"));
            }
            ui.label(RichText::new("Bright glow spreads with a Bloom effect in Camera & Screen.").size(10.5).color(theme::TEXT_DIM));
        });

        theme::section(ui, "Build it yourself");
        theme::card(ui, |ui| {
            if fx.graph.is_none() {
                ui.label(RichText::new("Open a node editor to wire textures, colours, noise and maths into this material.").size(11.0));
                if ui.button("Open in node editor").clicked() {
                    fx.graph = Some(MatGraph::starter());
                    self.mat_nodes_open = true;
                }
            } else {
                ui.horizontal(|ui| {
                    if ui.button(if self.mat_nodes_open { "Hide node editor" } else { "Show node editor" }).clicked() {
                        self.mat_nodes_open = !self.mat_nodes_open;
                    }
                    if ui.button("Remove node graph").clicked() {
                        fx.graph = None;
                    }
                });
                if let Some(g) = &fx.graph {
                    let (code, _) = g.compile();
                    use std::hash::{Hash, Hasher};
                    let mut h = std::collections::hash_map::DefaultHasher::new();
                    code.hash(&mut h);
                    if let Some(e) = self.renderer.as_ref().and_then(|r| r.material_errors.get(&h.finish())) {
                        ui.label(RichText::new(format!("Node graph problem: {e}")).size(10.5).color(Color32::from_rgb(255, 120, 110)));
                    }
                }
                // settings of the selected node
                if let (Some(g), Some(sel)) = (fx.graph.as_mut(), self.mnodes.selected) {
                    let mut del = false;
                    if let Some(n) = g.node_mut(sel) {
                        let info = n.kind.info();
                        ui.separator();
                        ui.label(RichText::new(info.title).strong());
                        ui.label(RichText::new(info.about).size(10.5).color(theme::TEXT_DIM));
                        for (i, (pn, mn, mx, _)) in info.params.iter().enumerate() {
                            ui.add(egui::Slider::new(&mut n.f[i], *mn..=*mx).text(*pn));
                        }
                        if info.color_param {
                            let mut c = [n.f[0], n.f[1], n.f[2]];
                            if ui.color_edit_button_rgb(&mut c).changed() {
                                n.f[0] = c[0];
                                n.f[1] = c[1];
                                n.f[2] = c[2];
                            }
                        }
                        if n.kind == MatNodeKind::Texture {
                            ui.horizontal(|ui| {
                                ui.label(n.path.as_deref().map(short).unwrap_or_else(|| "no picture".into()));
                                if ui.button("Pick…").clicked() {
                                    if let Some(p) = pick_image() {
                                        n.path = Some(p);
                                    }
                                }
                            });
                        }
                        if n.kind != MatNodeKind::Output {
                            del = ui.button("Delete node").clicked();
                        }
                    }
                    if del {
                        g.remove(sel);
                        self.mnodes.selected = None;
                    }
                }
            }
        });

        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(&mut self.preset_name_buf).hint_text("preset name").desired_width(140.0));
            if ui.button("💾 Save as my preset").on_hover_text("Reuse this material in any project").clicked() {
                let pname = if self.preset_name_buf.trim().is_empty() { name.clone() } else { self.preset_name_buf.trim().to_string() };
                self.user_presets.materials.retain(|(n, _)| *n != pname);
                self.user_presets.materials.push((pname.clone(), fx.clone()));
                match self.user_presets.save() {
                    Ok(_) => self.set_status(format!("Saved material preset “{pname}” for every project")),
                    Err(e) => self.set_error(format!("Could not save: {e}")),
                }
            }
        });
        if ui.button("Back to automatic").clicked() {
            self.ask(
                "Back to automatic?",
                &format!("Your settings, texture maps and node graph for “{name}” will be thrown away."),
                "Reset material",
                crate::ui::home::ConfirmAction::MaterialAuto(name.clone()),
            );
        }
        if fx != before {
            self.graph.mat_fx.insert(name, fx);
        }
    }

    fn material_presets_menu(&mut self, ui: &mut egui::Ui, name: &str) {
        ui.menu_button("Apply a material preset…", |ui| {
            let mut pick: Option<MaterialFx> = None;
            for (n, m) in builtin_material_presets() {
                if ui.button(n).clicked() {
                    pick = Some(m);
                    ui.close();
                }
            }
            if !self.user_presets.materials.is_empty() {
                ui.separator();
                ui.label(RichText::new("My presets").strong());
                for (n, m) in &self.user_presets.materials {
                    if ui.button(n).clicked() {
                        pick = Some(m.clone());
                        ui.close();
                    }
                }
            }
            if let Some(mut m) = pick {
                // keep maps the person already picked unless the preset brings its own
                if let Some(old) = self.graph.mat_fx.get(name) {
                    m.normal_map = m.normal_map.or(old.normal_map.clone());
                    m.height_map = m.height_map.or(old.height_map.clone());
                    m.metal_map = m.metal_map.or(old.metal_map.clone());
                    m.rough_map = m.rough_map.or(old.rough_map.clone());
                }
                self.graph.mat_fx.insert(name.to_string(), m);
            }
        });
    }

    /// The material node editor canvas (bottom panel).
    pub fn mat_node_canvas(&mut self, ui: &mut egui::Ui) {
        ui.set_min_size(ui.available_size());
        let rect = ui.max_rect().intersect(ui.clip_rect());
        let Some(name) = self.selected_material_name() else { return };
        let Some(mut g) = self.graph.mat_fx.get_mut(&name).and_then(|f| f.graph.take()) else {
            let p = ui.painter_at(rect);
            p.rect_filled(rect, 0.0, Color32::from_rgb(19, 20, 25));
            p.text(rect.center(), egui::Align2::CENTER_CENTER, "This material has no node graph — use “Open in node editor”.", egui::FontId::proportional(13.0), theme::TEXT_DIM);
            return;
        };
        let resp = ui.interact(rect, ui.scope_id().with("matcanvas"), Sense::click_and_drag());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, Color32::from_rgb(19, 20, 25));
        let z = self.mnodes.zoom.clamp(0.4, 2.0);
        let origin = rect.min.to_vec2() + self.mnodes.pan;
        let gs = 24.0 * z;
        let mut x = rect.left() + (origin.x - rect.left()).rem_euclid(gs);
        while x < rect.right() {
            painter.line_segment([pos2(x, rect.top()), pos2(x, rect.bottom())], Stroke::new(1.0, Color32::from_rgb(26, 28, 34)));
            x += gs;
        }
        painter.text(rect.left_top() + vec2(10.0, 8.0), egui::Align2::LEFT_TOP, format!("Material nodes · {name}"), egui::FontId::proportional(12.0), theme::TEXT_DIM);
        painter.text(rect.right_top() + vec2(-10.0, 8.0), egui::Align2::RIGHT_TOP, "drag outputs to inputs · right-click: add node · middle-drag: pan · wheel: zoom", egui::FontId::proportional(11.0), theme::TEXT_DIM);
        let to_screen = |p: [f32; 2]| pos2(origin.x + p[0] * z, origin.y + p[1] * z);
        struct G {
            id: u32,
            rect: Rect,
            ins: Vec<Pos2>,
            out: Option<Pos2>,
        }
        let geo: Vec<G> = g
            .nodes
            .iter()
            .map(|n| {
                let info = n.kind.info();
                let h = MN_HEAD + info.inputs.len().max(1) as f32 * MN_ROW + 6.0;
                let r = Rect::from_min_size(to_screen(n.pos), vec2(MN_W, h) * z);
                let ins = (0..info.inputs.len()).map(|i| pos2(r.left(), r.top() + (MN_HEAD + MN_ROW * (i as f32 + 0.5)) * z)).collect();
                let out = info.output.then(|| pos2(r.right(), r.top() + (MN_HEAD + MN_ROW * 0.5) * z));
                G { id: n.id, rect: r, ins, out }
            })
            .collect();
        let find = |id: u32| geo.iter().find(|q| q.id == id);
        let wire = |a: Pos2, b: Pos2, col: Color32| {
            let d = ((b.x - a.x).abs() * 0.5).max(40.0);
            painter.add(egui::epaint::CubicBezierShape::from_points_stroke([a, a + vec2(d, 0.0), b - vec2(d, 0.0), b], false, Color32::TRANSPARENT, Stroke::new(2.0 * z.max(0.7), col)));
        };
        for l in &g.links {
            if let (Some(a), Some(b)) = (find(l.from), find(l.to)) {
                if let (Some(pa), Some(pb)) = (a.out, b.ins.get(l.slot)) {
                    wire(pa, *pb, theme::ACCENT2);
                }
            }
        }
        let pointer = ui.input(|i| i.pointer.hover_pos());
        if let (Some(from), Some(mp)) = (self.mnodes.link_from, pointer) {
            if let Some(pa) = find(from).and_then(|q| q.out) {
                wire(pa, mp, theme::KEY);
            }
        }
        let mut hov_node = None;
        let mut hov_out = None;
        let mut hov_in = None;
        for q in &geo {
            let Some(n) = g.node(q.id) else { continue };
            let info = n.kind.info();
            let sel = self.mnodes.selected == Some(q.id);
            let head_col = match n.kind {
                MatNodeKind::Output => Color32::from_rgb(200, 80, 120),
                MatNodeKind::BaseTexture | MatNodeKind::Texture => Color32::from_rgb(200, 140, 60),
                MatNodeKind::Color | MatNodeKind::Value | MatNodeKind::Uv | MatNodeKind::Time => Color32::from_rgb(70, 140, 200),
                MatNodeKind::Fresnel | MatNodeKind::Noise | MatNodeKind::FacingUp => Color32::from_rgb(110, 90, 200),
                _ => Color32::from_rgb(70, 160, 110),
            };
            painter.rect_filled(q.rect, 7.0 * z, theme::BG2);
            let head = Rect::from_min_size(q.rect.min, vec2(q.rect.width(), MN_HEAD * z));
            painter.rect_filled(head, CornerRadius { nw: (7.0 * z) as u8, ne: (7.0 * z) as u8, sw: 0, se: 0 }, head_col);
            painter.rect_stroke(q.rect, 7.0 * z, Stroke::new(if sel { 2.0 } else { 1.0 }, if sel { Color32::WHITE } else { theme::BG3 }), egui::StrokeKind::Outside);
            painter.text(head.left_center() + vec2(8.0 * z, 0.0), egui::Align2::LEFT_CENTER, info.title, egui::FontId::proportional(12.0 * z), Color32::WHITE);
            if info.color_param {
                let c = Color32::from_rgb((n.f[0] * 255.0) as u8, (n.f[1] * 255.0) as u8, (n.f[2] * 255.0) as u8);
                painter.rect_filled(Rect::from_min_size(q.rect.min + vec2(MN_W - 30.0, MN_HEAD + 4.0) * z, vec2(22.0, 12.0) * z), 2.0, c);
            }
            if n.kind == MatNodeKind::Texture {
                let t = n.path.as_deref().map(short).unwrap_or_else(|| "no picture".into());
                painter.text(q.rect.left_bottom() + vec2(8.0, -8.0) * z, egui::Align2::LEFT_BOTTOM, t, egui::FontId::proportional(10.0 * z), theme::TEXT_DIM);
            }
            for (i, p) in q.ins.iter().enumerate() {
                painter.circle_filled(*p, 5.0 * z, theme::ACCENT2);
                painter.text(*p + vec2(9.0 * z, 0.0), egui::Align2::LEFT_CENTER, info.inputs[i], egui::FontId::proportional(10.5 * z), theme::TEXT_DIM);
                if pointer.map(|mp| (mp - *p).length() < 9.0).unwrap_or(false) {
                    hov_in = Some((q.id, i));
                }
            }
            if let Some(p) = q.out {
                painter.circle_filled(p, 5.0 * z, theme::KEY);
                if pointer.map(|mp| (mp - p).length() < 9.0).unwrap_or(false) {
                    hov_out = Some(q.id);
                }
            }
            if pointer.map(|mp| q.rect.contains(mp)).unwrap_or(false) {
                hov_node = Some(q.id);
            }
        }
        if resp.drag_started_by(egui::PointerButton::Primary) {
            if let Some(o) = hov_out {
                self.mnodes.link_from = Some(o);
            } else if let Some((nid, slot)) = hov_in {
                if let Some(i) = g.links.iter().position(|l| l.to == nid && l.slot == slot) {
                    let l = g.links.remove(i);
                    self.mnodes.link_from = Some(l.from);
                }
            } else if let Some(n) = hov_node {
                self.mnodes.dragging = Some(n);
                self.mnodes.selected = Some(n);
            }
        }
        if resp.dragged_by(egui::PointerButton::Primary) {
            if let Some(id) = self.mnodes.dragging {
                let d = resp.drag_delta() / z;
                if let Some(n) = g.node_mut(id) {
                    n.pos[0] += d.x;
                    n.pos[1] += d.y;
                }
            }
        }
        if resp.dragged_by(egui::PointerButton::Middle) || (resp.dragged_by(egui::PointerButton::Primary) && self.mnodes.dragging.is_none() && self.mnodes.link_from.is_none()) {
            self.mnodes.pan += resp.drag_delta();
        }
        if resp.drag_stopped() {
            if let Some(from) = self.mnodes.link_from.take() {
                if let Some((to, slot)) = hov_in {
                    g.connect(from, to, slot);
                }
            }
            self.mnodes.dragging = None;
        }
        if resp.clicked() {
            self.mnodes.selected = hov_node;
        }
        if resp.hovered() {
            let s = ui.input(|i| i.smooth_scroll_delta.y);
            if s != 0.0 {
                self.mnodes.zoom = (self.mnodes.zoom * (s * 0.0015).exp()).clamp(0.4, 2.0);
            }
            if ui.input(|i| i.key_pressed(egui::Key::Delete)) {
                if let Some(sel) = self.mnodes.selected.take() {
                    g.remove(sel);
                }
            }
        }
        if resp.secondary_clicked() {
            self.mnodes.add_menu_at = pointer;
        }
        if let Some(at) = self.mnodes.add_menu_at {
            let mut close = false;
            let area = egui::Area::new(ui.scope_id().with("mat_add")).fixed_pos(at).order(egui::Order::Foreground).show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    ui.label(RichText::new("Add node").strong());
                    for k in MatNodeKind::ADDABLE {
                        let info = k.info();
                        if ui.button(info.title).on_hover_text(info.about).clicked() {
                            let wp = (at.to_vec2() - rect.min.to_vec2() - self.mnodes.pan) / z;
                            let id = g.add(*k, [wp.x, wp.y]);
                            self.mnodes.selected = Some(id);
                            close = true;
                        }
                    }
                });
            });
            if close || (ui.input(|i| i.pointer.any_click()) && !area.response.rect.contains(pointer.unwrap_or(at)) && !resp.secondary_clicked()) {
                self.mnodes.add_menu_at = None;
            }
        }
        if let Some(f) = self.graph.mat_fx.get_mut(&name) {
            f.graph = Some(g);
        }
    }
}
