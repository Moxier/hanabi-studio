//! Effects workspace: preset gallery, node editor canvas and node inspector.

use crate::app::App;
use crate::effects::{presets, NodeKind, PKind, PVal, Socket};
use crate::ui::theme;
use egui::{pos2, vec2, Color32, CornerRadius, Pos2, Rect, RichText, Sense, Stroke};

const NODE_W: f32 = 176.0;
const HEADER_H: f32 = 26.0;
const SOCK_H: f32 = 22.0;
const GROUP_TITLE: f32 = 22.0;

fn category_color(cat: &str) -> Color32 {
    match cat {
        "Source" => Color32::from_rgb(90, 96, 112),
        "Scene look" => Color32::from_rgb(54, 140, 200),
        "Light" => Color32::from_rgb(214, 150, 50),
        "Colour" => Color32::from_rgb(200, 80, 150),
        "Camera" => Color32::from_rgb(110, 90, 200),
        "Stylise" => Color32::from_rgb(70, 170, 120),
        _ => Color32::from_rgb(120, 120, 120),
    }
}

fn socket_color(s: Socket) -> Color32 {
    match s {
        Socket::Image => theme::KEY,
        Socket::Fx => theme::ACCENT2,
    }
}

fn bezier(p: &egui::Painter, a: Pos2, b: Pos2, col: Color32, w: f32) {
    let d = ((b.x - a.x).abs() * 0.5).max(40.0);
    let shape = egui::epaint::CubicBezierShape::from_points_stroke([a, a + vec2(d, 0.0), b - vec2(d, 0.0), b], false, Color32::TRANSPARENT, Stroke::new(w, col));
    p.add(shape);
}

impl App {
    pub fn effects_graph_ws(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        egui::Panel::left("fx_presets").resizable(true).default_size(250.0).size_range(200.0..=420.0).show(ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| self.preset_gallery(ui));
        });
        egui::Panel::right("fx_inspector").resizable(true).default_size(300.0).size_range(240.0..=480.0).show(ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| self.node_inspector(ui));
        });
        let target = (ui.available_height() * 0.42).clamp(220.0, 520.0);
        let panel = egui::Panel::bottom("fx_graph").resizable(true).default_size(target).size_range(160.0..=900.0);
        panel.frame(egui::Frame::new().fill(theme::BG0)).show(ui, |ui| {
            self.node_canvas(ui);
            self.preset_popup(ui);
        });

        egui::CentralPanel::default().frame(egui::Frame::new().fill(theme::BG0)).show(ui, |ui| {
            let rect = ui.max_rect();
            let bar = Rect::from_min_size(rect.min, vec2(rect.width(), 30.0));
            ui.scope_builder(egui::UiBuilder::new().max_rect(bar.shrink2(vec2(8.0, 3.0))), |ui| {
                ui.horizontal_centered(|ui| self.fx_view_switch(ui));
            });
            let vr = Rect::from_min_max(rect.min + vec2(0.0, 30.0), rect.max);
            self.viewport(ui, frame, vr, false);
        });
    }

    fn preset_gallery(&mut self, ui: &mut egui::Ui) {
        theme::section(ui, "Graph");
        ui.label(RichText::new("Every graph starts from Scene Render → Output. Add nodes below, or pull a ready-made look from the preset drawer.").size(11.5).color(theme::TEXT_DIM));
        if ui.button("Clear graph (Scene Render → Output)").clicked() {
            self.ask("Clear the whole graph?", "Every effect node will be removed, leaving only Scene Render → Output.", "Clear", crate::ui::home::ConfirmAction::ClearGraph);
        }
        ui.add_space(4.0);
        egui::CollapsingHeader::new(RichText::new("Preset drawer").strong())
            .id_salt("preset_drawer")
            .default_open(false)
            .show(ui, |ui| {
                ui.label(RichText::new("Click to add a preset's nodes as a group. Right-click to replace the whole graph.").size(11.0).color(theme::TEXT_DIM));
                for p in presets() {
                    let resp = egui::Frame::new()
                        .fill(theme::BG2)
                        .stroke(Stroke::new(1.0, theme::BG3))
                        .corner_radius(CornerRadius::same(7))
                        .inner_margin(egui::Margin::same(7))
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.label(RichText::new(p.name).strong().color(Color32::WHITE));
                            ui.label(RichText::new(p.about).size(10.5).color(theme::TEXT_DIM));
                        })
                        .response
                        .interact(Sense::click());
                    if resp.clicked() {
                        self.apply_preset(p.name, false);
                    }
                    resp.context_menu(|ui| {
                        if ui.button("Add to graph").clicked() {
                            self.apply_preset(p.name, false);
                            ui.close();
                        }
                        if ui.button("Replace whole graph").clicked() {
                            self.ask(
                                "Replace the whole graph?",
                                &format!("Every node will be replaced by the “{}” preset.", p.name),
                                "Replace",
                                crate::ui::home::ConfirmAction::ReplaceGraph(p.name.to_string()),
                            );
                            ui.close();
                        }
                    });
                    if resp.hovered() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    }
                    ui.add_space(2.0);
                }
            });
        ui.add_space(6.0);
        theme::section(ui, "Add a node");
        let mut cats: Vec<&str> = Vec::new();
        for k in NodeKind::ALL {
            let c = k.info().category;
            if !cats.contains(&c) && c != "Source" {
                cats.push(c);
            }
        }
        for c in cats {
            ui.label(RichText::new(c).size(11.0).color(category_color(c)));
            ui.horizontal_wrapped(|ui| {
                for k in NodeKind::ALL.iter().filter(|k| k.info().category == c) {
                    let info = k.info();
                    if ui.button(info.title).on_hover_text(info.about).clicked() {
                        let id = self.graph.append_before_output(*k);
                        self.graph.preset_name = "Custom".into();
                        self.nodes.selected = Some(id);
                        self.nodes.multi = vec![id];
                    }
                }
            });
        }
        ui.add_space(6.0);
        ui.label(RichText::new("Not compatible with MME effect files: everything here is built in and runs on the GPU.").size(10.5).color(theme::TEXT_DIM));
    }

    fn node_inspector(&mut self, ui: &mut egui::Ui) {
        if self.nodes.multi.len() > 1 {
            theme::section(ui, "Selection");
            theme::card(ui, |ui| {
                ui.label(format!("{} nodes selected", self.nodes.multi.len()));
                ui.horizontal(|ui| {
                    if ui.button("Group  (Ctrl+G)").clicked() {
                        self.group_selected();
                    }
                    if ui.button("Delete").clicked() {
                        self.delete_selected_nodes();
                    }
                });
            });
        }
        if let Some(gid) = self.nodes.selected.and_then(|s| self.graph.group_of(s).map(|g| g.id)) {
            theme::section(ui, "Group");
            let mut ungroup = false;
            theme::card(ui, |ui| {
                if let Some(g) = self.graph.groups.iter_mut().find(|g| g.id == gid) {
                    ui.horizontal(|ui| {
                        ui.label("Name");
                        ui.text_edit_singleline(&mut g.name);
                    });
                    ui.horizontal(|ui| {
                        let mut c = Color32::from_rgb(g.color[0], g.color[1], g.color[2]);
                        if ui.color_edit_button_srgba(&mut c).changed() {
                            g.color = [c.r(), c.g(), c.b()];
                        }
                        ui.checkbox(&mut g.collapsed, "Folded");
                        ungroup = ui.button("Ungroup").clicked();
                    });
                    let ids = g.nodes.clone();
                    let mut all_on = true;
                    for id in &ids {
                        if let Some(n) = self.graph.nodes.iter().find(|n| n.id == *id) {
                            all_on &= n.enabled;
                        }
                    }
                    let mut on = all_on;
                    if ui.checkbox(&mut on, "Whole group on").changed() {
                        for n in self.graph.nodes.iter_mut().filter(|n| ids.contains(&n.id)) {
                            n.enabled = on;
                        }
                    }
                }
            });
            if ungroup {
                self.graph.ungroup(gid);
            }
        }
        theme::section(ui, "Node");
        let Some(id) = self.nodes.selected else {
            ui.label(RichText::new("Click a node in the graph to edit it.").color(theme::TEXT_DIM));
            ui.add_space(8.0);
            theme::section(ui, "How it works");
            ui.label("• Scene look nodes (blue sockets) plug into Scene Render and change how the stage is drawn: shading style, particles, beams, water, sky, mirror floor.");
            ui.label("• Image nodes (yellow sockets) are chained from Scene Render to Output and run one after another.");
            ui.label("• Drag from a socket to another to connect. Drag on empty space to box-select, Shift-click to add to the selection, Ctrl+G to group, double-click a group title to fold it. Right-click the canvas to add nodes. Delete removes the selection.");
            return;
        };
        let Some(node) = self.graph.node_mut(id) else {
            self.nodes.selected = None;
            return;
        };
        let info = node.kind.info();
        let mut changed = false;
        theme::card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(info.title).strong().size(15.0).color(category_color(info.category)));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if !matches!(node.kind, NodeKind::SceneRender | NodeKind::Output) {
                        changed |= ui.checkbox(&mut node.enabled, "On").changed();
                    }
                });
            });
            ui.label(RichText::new(info.about).size(11.5).color(theme::TEXT_DIM));
            ui.separator();
            for (pi, spec) in info.params.iter().enumerate() {
                let Some(v) = node.values.get_mut(pi) else { continue };
                ui.horizontal(|ui| {
                    match (&spec.kind, v) {
                        (PKind::Float(min, max, _), PVal::F(x)) => {
                            let r = ui.add(egui::Slider::new(x, *min..=*max).text(spec.name).logarithmic(*max / min.max(0.001) > 200.0 && *min > 0.0));
                            changed |= r.changed();
                        }
                        (PKind::Color(_), PVal::C(c)) => {
                            changed |= ui.color_edit_button_rgb(c).changed();
                            ui.label(spec.name);
                        }
                        (PKind::Choice(opts, _), PVal::I(i)) => {
                            egui::ComboBox::from_label(spec.name).selected_text(opts.get(*i).copied().unwrap_or("?")).show_ui(ui, |ui| {
                                for (k, o) in opts.iter().enumerate() {
                                    changed |= ui.selectable_value(i, k, *o).changed();
                                }
                            });
                        }
                        (PKind::Toggle(_), PVal::B(b)) => {
                            changed |= ui.checkbox(b, spec.name).changed();
                        }
                        _ => {}
                    }
                })
                .response
                .on_hover_text(spec.help);
            }
            if info.params.is_empty() {
                ui.label(RichText::new("No settings.").color(theme::TEXT_DIM));
            }
        });
        if changed {
            self.graph.preset_name = format!("{} (edited)", self.graph.preset_name.trim_end_matches(" (edited)"));
        }
        if self.graph.node(id).map(|n| n.kind) == Some(NodeKind::RayLighting) {
            self.material_types(ui);
        }
        if !matches!(self.graph.node(id).map(|n| n.kind), Some(NodeKind::SceneRender) | Some(NodeKind::Output)) && ui.button("🗑 Remove node").clicked() {
            self.graph.remove(id);
            self.nodes.selected = None;
            self.nodes.multi.retain(|x| *x != id);
        }
        ui.add_space(8.0);
        theme::section(ui, "Performance");
        if let Some(r) = &self.renderer {
            ui.label(format!("Frame: {:.2} ms · {} draw calls · {} particles", r.stats.gpu_ms, r.stats.draw_calls, r.stats.particles));
        }
    }

    /// Material type picker for Ray Lighting: one row per material of every model on stage.
    pub fn material_types(&mut self, ui: &mut egui::Ui) {
        use crate::effects::MatPreset;
        theme::section(ui, "Materials");
        let auto = self.graph.fx_of(NodeKind::RayLighting).map(|n| n.b("Auto materials")).unwrap_or(true);
        ui.label(RichText::new("What each part is made of. “Auto” guesses from the material's name.").size(11.0).color(theme::TEXT_DIM));
        let models: Vec<(String, Vec<String>)> = self
            .scene
            .models
            .iter()
            .map(|m| (m.name().to_string(), m.data.materials.iter().map(|x| x.name.clone()).collect()))
            .collect();
        for (mi, (mname, mats)) in models.iter().enumerate() {
            egui::CollapsingHeader::new(RichText::new(mname).strong()).id_salt(("mats", mi)).default_open(mi == 0).show(ui, |ui| {
                egui::Grid::new(("matgrid", mi)).num_columns(2).striped(true).show(ui, |ui| {
                    let mut seen = std::collections::HashSet::new();
                    for name in mats {
                        if !seen.insert(name.clone()) {
                            continue;
                        }
                        ui.label(RichText::new(name).size(11.5));
                        let cur = self.graph.materials.get(name).copied();
                        let guess = if auto { MatPreset::guess(name) } else { MatPreset::Cloth };
                        let text = match cur {
                            Some(p) => p.label().to_string(),
                            None => format!("Auto ({})", guess.label()),
                        };
                        egui::ComboBox::from_id_salt(("mat", mi, name)).width(140.0).selected_text(text).show_ui(ui, |ui| {
                            if ui.selectable_label(cur.is_none(), format!("Auto ({})", guess.label())).clicked() {
                                self.graph.materials.remove(name);
                            }
                            for p in MatPreset::ALL {
                                if ui.selectable_label(cur == Some(p), p.label()).clicked() {
                                    self.graph.materials.insert(name.clone(), p);
                                }
                            }
                        });
                        ui.end_row();
                    }
                });
            });
        }
        if !self.graph.materials.is_empty() && ui.small_button("Reset all to Auto").clicked() {
            self.graph.materials.clear();
        }
    }

    fn node_canvas(&mut self, ui: &mut egui::Ui) {
        ui.set_min_size(ui.available_size());
        let rect = ui.max_rect().intersect(ui.clip_rect());
        let size_changed = (rect.size() - self.nodes.fit_size).length() > 4.0;
        if (self.nodes.fit_pending || (size_changed && !self.nodes.touched)) && !self.graph.nodes.is_empty() && rect.height() > 120.0 && rect.width() > 300.0 {
            self.nodes.fit_size = rect.size();
            self.nodes.fit_pending = false;
            let (mut mn, mut mx) = (vec2(f32::MAX, f32::MAX), vec2(f32::MIN, f32::MIN));
            for n in &self.graph.nodes {
                let h = HEADER_H + n.kind.info().inputs.len().max(1) as f32 * SOCK_H + 8.0;
                mn = mn.min(vec2(n.pos[0], n.pos[1] - GROUP_TITLE));
                mx = mx.max(vec2(n.pos[0] + NODE_W, n.pos[1] + h));
            }
            let size = (mx - mn).max(vec2(1.0, 1.0));
            let avail = rect.size() - vec2(40.0, 80.0);
            let z = (avail.x / size.x).min(avail.y / size.y).clamp(0.3, 1.1);
            self.nodes.zoom = z;
            let center = (mn + mx) * 0.5;
            self.nodes.pan = rect.size() * 0.5 + vec2(0.0, 16.0) - center * z;
        }
        let resp = ui.interact(rect, ui.scope_id().with("canvas"), Sense::click_and_drag());
        let painter = ui.painter_at(rect);
        // background grid
        painter.rect_filled(rect, 0.0, Color32::from_rgb(19, 20, 25));
        let z = self.nodes.zoom.clamp(0.4, 2.0);
        let origin = rect.min.to_vec2() + self.nodes.pan;
        let gs = 24.0 * z;
        let mut x = rect.left() + (origin.x - rect.left()).rem_euclid(gs);
        while x < rect.right() {
            painter.line_segment([pos2(x, rect.top()), pos2(x, rect.bottom())], Stroke::new(1.0, Color32::from_rgb(26, 28, 34)));
            x += gs;
        }
        let mut y = rect.top() + (origin.y - rect.top()).rem_euclid(gs);
        while y < rect.bottom() {
            painter.line_segment([pos2(rect.left(), y), pos2(rect.right(), y)], Stroke::new(1.0, Color32::from_rgb(26, 28, 34)));
            y += gs;
        }
        painter.text(rect.left_top() + vec2(10.0, 10.0), egui::Align2::LEFT_TOP, format!("Node graph · {}", self.graph.preset_name), egui::FontId::proportional(12.0), theme::TEXT_DIM);
        painter.text(
            rect.right_bottom() + vec2(-10.0, -8.0),
            egui::Align2::RIGHT_BOTTOM,
            "drag empty space: box select · shift: add · middle-drag: pan · wheel: zoom · right-click: add node · Ctrl+G: group",
            egui::FontId::proportional(11.0),
            theme::TEXT_DIM,
        );

        let to_screen = |p: [f32; 2]| pos2(origin.x + p[0] * z, origin.y + p[1] * z);
        // Collapsed groups draw as a single tile at their members' top-left.
        let mut tiles: Vec<(u32, Rect)> = Vec::new();
        for g in self.graph.groups.iter().filter(|g| g.collapsed) {
            let mut mn = vec2(f32::MAX, f32::MAX);
            for id in &g.nodes {
                if let Some(n) = self.graph.node(*id) {
                    mn = mn.min(vec2(n.pos[0], n.pos[1]));
                }
            }
            if mn.x < f32::MAX {
                tiles.push((g.id, Rect::from_min_size(to_screen([mn.x, mn.y]), vec2(NODE_W, HEADER_H + SOCK_H + 8.0) * z)));
            }
        }
        let hidden_in = |id: u32| -> Option<(u32, Rect)> {
            let g = self.graph.group_of(id)?;
            tiles.iter().find(|(gid, _)| *gid == g.id).copied()
        };
        // geometry of every node
        struct G {
            id: u32,
            rect: Rect,
            inputs: Vec<(Pos2, Socket)>,
            output: Option<(Pos2, Socket)>,
            hidden: Option<u32>,
        }
        let geo: Vec<G> = self
            .graph
            .nodes
            .iter()
            .map(|n| {
                let info = n.kind.info();
                if let Some((gid, t)) = hidden_in(n.id) {
                    let mid = t.top() + (HEADER_H + SOCK_H * 0.5) * z;
                    return G {
                        id: n.id,
                        rect: t,
                        inputs: info.inputs.iter().map(|(_, s)| (pos2(t.left(), mid), *s)).collect(),
                        output: info.output.map(|s| (pos2(t.right(), mid), s)),
                        hidden: Some(gid),
                    };
                }
                let rows = info.inputs.len().max(1) as f32;
                let h = HEADER_H + rows * SOCK_H + 8.0;
                let r = Rect::from_min_size(to_screen(n.pos), vec2(NODE_W, h) * z);
                let inputs = info.inputs.iter().enumerate().map(|(i, (_, s))| (pos2(r.left(), r.top() + (HEADER_H + SOCK_H * (i as f32 + 0.5)) * z), *s)).collect();
                let output = info.output.map(|s| (pos2(r.right(), r.top() + (HEADER_H + SOCK_H * 0.5) * z), s));
                G { id: n.id, rect: r, inputs, output, hidden: None }
            })
            .collect();
        let find = |id: u32| geo.iter().find(|g| g.id == id);
        let pointer = ui.input(|i| i.pointer.hover_pos());
        let shift = ui.input(|i| i.modifiers.shift || i.modifiers.command);

        // group frames (behind nodes)
        let mut hovered_group_title: Option<u32> = None;
        for grp in &self.graph.groups {
            let col = Color32::from_rgb(grp.color[0], grp.color[1], grp.color[2]);
            let (frame_rect, title_rect) = if let Some((_, t)) = tiles.iter().find(|(g, _)| *g == grp.id) {
                (t.expand(4.0 * z), Rect::from_min_size(t.min - vec2(4.0, GROUP_TITLE) * z, vec2(t.width() + 8.0 * z, GROUP_TITLE * z)))
            } else {
                let mut r = Rect::NOTHING;
                for id in &grp.nodes {
                    if let Some(g) = find(*id) {
                        r = r.union(g.rect);
                    }
                }
                if r == Rect::NOTHING {
                    continue;
                }
                let r = r.expand(12.0 * z);
                let r = Rect::from_min_max(r.min - vec2(0.0, GROUP_TITLE * z), r.max);
                (r, Rect::from_min_size(r.min, vec2(r.width(), GROUP_TITLE * z)))
            };
            let any_sel = grp.nodes.iter().any(|n| self.nodes.multi.contains(n));
            painter.rect_filled(frame_rect, 9.0 * z, Color32::from_rgba_unmultiplied(col.r(), col.g(), col.b(), 22));
            painter.rect_stroke(frame_rect, 9.0 * z, Stroke::new(if any_sel { 1.6 } else { 1.0 }, col.linear_multiply(if any_sel { 1.0 } else { 0.6 })), egui::StrokeKind::Inside);
            painter.rect_filled(title_rect, CornerRadius { nw: (9.0 * z) as u8, ne: (9.0 * z) as u8, sw: 0, se: 0 }, col.linear_multiply(0.45));
            let arrow = if grp.collapsed { "▶" } else { "▼" };
            painter.text(
                title_rect.left_center() + vec2(8.0 * z, 0.0),
                egui::Align2::LEFT_CENTER,
                format!("{arrow}  {}  ({})", grp.name, grp.nodes.len()),
                egui::FontId::proportional(11.5 * z.max(0.8)),
                Color32::WHITE,
            );
            if let Some(mp) = pointer {
                if title_rect.contains(mp) {
                    hovered_group_title = Some(grp.id);
                }
            }
        }

        // links (internal links of collapsed groups are hidden)
        for l in &self.graph.links {
            if let (Some(a), Some(b)) = (find(l.from), find(l.to)) {
                if a.hidden.is_some() && a.hidden == b.hidden {
                    continue;
                }
                if let (Some((pa, s)), Some((pb, _))) = (a.output, b.inputs.get(l.slot)) {
                    bezier(&painter, pa, *pb, socket_color(s).linear_multiply(0.85), 2.2 * z.max(0.7));
                }
            }
        }
        // dragging a new link
        if let (Some(from), Some(mp)) = (self.nodes.link_from, pointer) {
            if let Some((pa, s)) = find(from).and_then(|g| g.output) {
                bezier(&painter, pa, mp, socket_color(s), 2.0);
            }
        }

        // collapsed group tiles
        for (gid, t) in &tiles {
            let Some(grp) = self.graph.groups.iter().find(|g| g.id == *gid) else { continue };
            let col = Color32::from_rgb(grp.color[0], grp.color[1], grp.color[2]);
            painter.rect_filled(t.translate(vec2(2.0, 3.0)), 7.0 * z, Color32::from_black_alpha(90));
            painter.rect_filled(*t, 7.0 * z, theme::BG2);
            let head = Rect::from_min_size(t.min, vec2(t.width(), HEADER_H * z));
            painter.rect_filled(head, CornerRadius { nw: (7.0 * z) as u8, ne: (7.0 * z) as u8, sw: 0, se: 0 }, col.linear_multiply(0.8));
            painter.text(head.left_center() + vec2(8.0 * z, 0.0), egui::Align2::LEFT_CENTER, &grp.name, egui::FontId::proportional(12.5 * z), Color32::WHITE);
            painter.text(pos2(t.center().x, t.bottom() - 10.0 * z), egui::Align2::CENTER_CENTER, format!("{} nodes · double-click title to open", grp.nodes.len()), egui::FontId::proportional(10.0 * z), Color32::from_gray(170));
        }

        // nodes
        let mut hovered_node = None;
        let mut hovered_out = None;
        let mut hovered_in: Option<(u32, usize)> = None;
        for g in geo.iter().filter(|g| g.hidden.is_none()) {
            let Some(n) = self.graph.node(g.id) else { continue };
            let info = n.kind.info();
            let sel = self.nodes.multi.contains(&g.id);
            let primary = self.nodes.selected == Some(g.id);
            let cc = category_color(info.category);
            painter.rect_filled(g.rect.translate(vec2(2.0, 3.0)), 7.0 * z, Color32::from_black_alpha(90));
            painter.rect_filled(g.rect, 7.0 * z, theme::BG2);
            let head = Rect::from_min_size(g.rect.min, vec2(g.rect.width(), HEADER_H * z));
            painter.rect_filled(head, CornerRadius { nw: (7.0 * z) as u8, ne: (7.0 * z) as u8, sw: 0, se: 0 }, if n.enabled { cc } else { cc.linear_multiply(0.35) });
            let stroke = if primary {
                Stroke::new(2.2, Color32::WHITE)
            } else if sel {
                Stroke::new(1.8, theme::ACCENT)
            } else {
                Stroke::new(1.0, theme::BG3)
            };
            painter.rect_stroke(g.rect, 7.0 * z, stroke, egui::StrokeKind::Outside);
            painter.text(head.left_center() + vec2(8.0 * z, 0.0), egui::Align2::LEFT_CENTER, info.title, egui::FontId::proportional(12.5 * z), Color32::WHITE);
            if !n.enabled {
                painter.text(head.right_center() - vec2(8.0 * z, 0.0), egui::Align2::RIGHT_CENTER, "off", egui::FontId::proportional(10.5 * z), Color32::from_white_alpha(160));
            }
            for (i, (p, s)) in g.inputs.iter().enumerate() {
                painter.circle_filled(*p, 5.5 * z, socket_color(*s));
                painter.circle_stroke(*p, 5.5 * z, Stroke::new(1.0, Color32::BLACK));
                painter.text(*p + vec2(10.0 * z, 0.0), egui::Align2::LEFT_CENTER, info.inputs[i].0, egui::FontId::proportional(11.0 * z), theme::TEXT_DIM);
                if let Some(mp) = pointer {
                    if (mp - *p).length() < 9.0 {
                        hovered_in = Some((g.id, i));
                    }
                }
            }
            if let Some((p, s)) = g.output {
                painter.circle_filled(p, 5.5 * z, socket_color(s));
                painter.circle_stroke(p, 5.5 * z, Stroke::new(1.0, Color32::BLACK));
                let lbl = if s == Socket::Fx { "look" } else { "image" };
                painter.text(p - vec2(10.0 * z, 0.0), egui::Align2::RIGHT_CENTER, lbl, egui::FontId::proportional(11.0 * z), theme::TEXT_DIM);
                if let Some(mp) = pointer {
                    if (mp - p).length() < 9.0 {
                        hovered_out = Some(g.id);
                    }
                }
            }
            let summary = match n.kind {
                NodeKind::Particles => format!("{} · {:.0}", crate::effects::PARTICLE_KINDS.get(n.i("Kind")).unwrap_or(&""), n.f("Amount")),
                NodeKind::ShadingStyle => crate::effects::STYLES.get(n.i("Style")).unwrap_or(&"").to_string(),
                NodeKind::Tonemap => crate::effects::TONEMAPS.get(n.i("Curve")).unwrap_or(&"").to_string(),
                NodeKind::Bloom => format!("intensity {:.2}", n.f("Intensity")),
                _ => String::new(),
            };
            if !summary.is_empty() {
                painter.text(pos2(g.rect.center().x, g.rect.bottom() - 9.0 * z), egui::Align2::CENTER_CENTER, summary, egui::FontId::proportional(10.5 * z), Color32::from_gray(170));
            }
            if let Some(mp) = pointer {
                if g.rect.contains(mp) {
                    hovered_node = Some(g.id);
                }
            }
        }
        let hovered_tile = pointer.and_then(|mp| tiles.iter().find(|(_, t)| t.contains(mp)).map(|(g, _)| *g));
        let group_target = hovered_group_title.or(hovered_tile);

        // toolbar (top right)
        let mut tx = rect.right() - 8.0;
        let ty = rect.top() + 6.0;
        let mut tool = |label: &str, enabled: bool, ui: &mut egui::Ui| -> bool {
            let w = (20.0 + label.chars().count() as f32 * 7.2).max(56.0);
            let r = Rect::from_min_size(pos2(tx - w, ty), vec2(w, 22.0));
            tx -= w + 4.0;
            ui.put(r, egui::Button::new(RichText::new(label).size(11.5))).clicked() && enabled
        };
        let n_sel = self.nodes.multi.len();
        let cur_group = self.nodes.selected.and_then(|s| self.graph.group_of(s).map(|g| g.id));
        if tool("Fit view", true, ui) {
            self.nodes.fit_pending = true;
        }
        if tool("Presets…", true, ui) {
            let px = pointer.map(|p| p.x).unwrap_or(rect.right());
            self.nodes.preset_menu_at = Some(pos2((px - 200.0).max(rect.left()), ty + 26.0));
        }
        if tool("Ungroup", cur_group.is_some(), ui) {
            if let Some(g) = cur_group {
                self.graph.ungroup(g);
            }
        }
        if tool(&format!("Group ({n_sel})"), n_sel > 0, ui) {
            self.group_selected();
        }
        let over_toolbar = pointer.map(|p| p.y < ty + 24.0 && p.x > tx).unwrap_or(false);

        // interactions
        if resp.drag_started() || (resp.hovered() && ui.input(|i| i.smooth_scroll_delta.y != 0.0)) {
            self.nodes.touched = true;
        }
        if resp.drag_started_by(egui::PointerButton::Primary) && !over_toolbar {
            if let Some(o) = hovered_out {
                self.nodes.link_from = Some(o);
            } else if let Some((nid, slot)) = hovered_in {
                if let Some(pos) = self.graph.links.iter().position(|l| l.to == nid && l.slot == slot) {
                    let l = self.graph.links.remove(pos);
                    self.nodes.link_from = Some(l.from);
                }
            } else if let Some(n) = hovered_node {
                if !self.nodes.multi.contains(&n) {
                    if !shift {
                        self.nodes.multi.clear();
                    }
                    self.nodes.multi.push(n);
                }
                self.nodes.selected = Some(n);
                self.nodes.dragging_node = Some(n);
            } else if let Some(gid) = group_target {
                // drag a whole group by its title (or its collapsed tile)
                if let Some(g) = self.graph.groups.iter().find(|g| g.id == gid) {
                    self.nodes.multi = g.nodes.clone();
                    self.nodes.selected = g.nodes.first().copied();
                    self.nodes.dragging_node = g.nodes.first().copied();
                }
            } else {
                self.nodes.box_start = ui.input(|i| i.pointer.press_origin());
                if !shift {
                    self.nodes.multi.clear();
                    self.nodes.selected = None;
                }
            }
        }
        if resp.dragged_by(egui::PointerButton::Primary) && self.nodes.dragging_node.is_some() {
            let d = resp.drag_delta() / z;
            let ids = self.nodes.multi.clone();
            for id in ids {
                if let Some(node) = self.graph.node_mut(id) {
                    node.pos[0] += d.x;
                    node.pos[1] += d.y;
                }
            }
        }
        if let (Some(b0), Some(mp)) = (self.nodes.box_start, pointer) {
            let r = Rect::from_two_pos(b0, mp);
            painter.rect_filled(r, 2.0, theme::ACCENT2.linear_multiply(0.12));
            painter.rect_stroke(r, 2.0, Stroke::new(1.0, theme::ACCENT2), egui::StrokeKind::Inside);
        }
        if resp.dragged_by(egui::PointerButton::Middle) {
            self.nodes.pan += resp.drag_delta();
        }
        if resp.drag_stopped() {
            if let Some(from) = self.nodes.link_from.take() {
                if let Some((to, slot)) = hovered_in {
                    self.graph.connect(from, to, slot);
                    self.graph.preset_name = "Custom".into();
                } else if let Some(to) = hovered_node {
                    let out = self.graph.node(from).and_then(|n| n.kind.info().output);
                    if let Some(tn) = self.graph.node(to) {
                        if let Some(slot) = tn.kind.info().inputs.iter().position(|(_, s)| Some(*s) == out) {
                            self.graph.connect(from, to, slot);
                            self.graph.preset_name = "Custom".into();
                        }
                    }
                }
            }
            if let (Some(b0), Some(mp)) = (self.nodes.box_start.take(), pointer) {
                let r = Rect::from_two_pos(b0, mp);
                for g in &geo {
                    if r.intersects(g.rect) && !self.nodes.multi.contains(&g.id) {
                        // a collapsed group is selected as a whole
                        self.nodes.multi.push(g.id);
                    }
                }
                self.nodes.selected = self.nodes.multi.last().copied();
            }
            self.nodes.dragging_node = None;
        }
        if resp.double_clicked() {
            if let Some(gid) = group_target {
                if let Some(g) = self.graph.groups.iter_mut().find(|g| g.id == gid) {
                    g.collapsed = !g.collapsed;
                }
            }
        } else if resp.clicked() && !over_toolbar {
            match (hovered_node, group_target) {
                (Some(n), _) => {
                    if shift {
                        if let Some(i) = self.nodes.multi.iter().position(|x| *x == n) {
                            self.nodes.multi.remove(i);
                        } else {
                            self.nodes.multi.push(n);
                        }
                    } else {
                        self.nodes.multi = vec![n];
                    }
                    self.nodes.selected = Some(n);
                }
                (None, Some(gid)) => {
                    if let Some(g) = self.graph.groups.iter().find(|g| g.id == gid) {
                        self.nodes.multi = g.nodes.clone();
                        self.nodes.selected = g.nodes.first().copied();
                    }
                }
                _ => {
                    if !shift {
                        self.nodes.multi.clear();
                        self.nodes.selected = None;
                    }
                }
            }
        }
        if resp.hovered() {
            let s = ui.input(|i| i.smooth_scroll_delta.y);
            if s != 0.0 {
                let old = self.nodes.zoom;
                let new = (old * (s * 0.0015).exp()).clamp(0.4, 2.0);
                if let Some(mp) = pointer {
                    let world = (mp.to_vec2() - rect.min.to_vec2() - self.nodes.pan) / old;
                    self.nodes.pan = mp.to_vec2() - rect.min.to_vec2() - world * new;
                }
                self.nodes.zoom = new;
            }
        }
        if resp.secondary_clicked() {
            self.nodes.add_menu_at = pointer;
        }
        if let Some(at) = self.nodes.add_menu_at {
            let mut close = false;
            let area = egui::Area::new(ui.scope_id().with("addmenu")).fixed_pos(at).order(egui::Order::Foreground).show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style()).show(ui, |ui| {
                    if !self.nodes.multi.is_empty() {
                        if ui.button(format!("Group {} selected node(s)", self.nodes.multi.len())).clicked() {
                            self.group_selected();
                            close = true;
                        }
                        if ui.button(format!("Delete {} selected node(s)", self.nodes.multi.len())).clicked() {
                            self.delete_selected_nodes();
                            close = true;
                        }
                        ui.separator();
                    }
                    ui.label(RichText::new("Add node").strong());
                    for k in NodeKind::ALL.iter().filter(|k| !matches!(k, NodeKind::SceneRender | NodeKind::Output)) {
                        let info = k.info();
                        if ui.button(RichText::new(info.title).color(category_color(info.category))).on_hover_text(info.about).clicked() {
                            let wp = (at.to_vec2() - rect.min.to_vec2() - self.nodes.pan) / z;
                            let id = self.graph.add(*k, [wp.x, wp.y]);
                            self.nodes.selected = Some(id);
                            self.nodes.multi = vec![id];
                            self.graph.preset_name = "Custom".into();
                            close = true;
                        }
                    }
                });
            });
            if close || (ui.input(|i| i.pointer.any_click()) && !area.response.rect.contains(pointer.unwrap_or(at)) && !resp.secondary_clicked()) {
                self.nodes.add_menu_at = None;
            }
        }
    }

    fn preset_popup(&mut self, ui: &mut egui::Ui) {
        let Some(at) = self.nodes.preset_menu_at else { return };
        let mut close = false;
        let mut pick: Option<(&'static str, bool)> = None;
        let area = egui::Area::new(ui.scope_id().with("presetmenu")).fixed_pos(at).order(egui::Order::Foreground).show(ui.ctx(), |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.set_width(250.0);
                ui.label(RichText::new("Add a preset to the graph").strong());
                for p in presets() {
                    if ui.button(p.name).on_hover_text(p.about).clicked() {
                        pick = Some((p.name, false));
                        close = true;
                    }
                }
                ui.separator();
                ui.label(RichText::new("Right-click a preset in the drawer to replace the graph.").size(10.5).color(theme::TEXT_DIM));
            });
        });
        if let Some((name, replace)) = pick {
            self.apply_preset(name, replace);
        }
        let pointer = ui.input(|i| i.pointer.interact_pos());
        if close || (ui.input(|i| i.pointer.any_pressed()) && pointer.map(|p| !area.response.rect.contains(p) && p.y > at.y).unwrap_or(false)) {
            self.nodes.preset_menu_at = None;
        }
    }

    pub fn group_selected(&mut self) {
        let ids = self.nodes.multi.clone();
        let n = self.graph.groups.len() + 1;
        if self.graph.group(&ids, &format!("Group {n}")).is_some() {
            self.set_status(format!("Grouped {} node(s). Double-click the group title to fold it.", ids.len()));
        }
    }

    pub fn delete_selected_nodes(&mut self) {
        let ids = std::mem::take(&mut self.nodes.multi);
        for id in ids {
            self.graph.remove(id);
        }
        self.nodes.selected = None;
        self.graph.preset_name = "Custom".into();
    }

    pub fn apply_preset(&mut self, name: &str, replace: bool) -> bool {
        let Some(p) = presets().into_iter().find(|p| p.name.eq_ignore_ascii_case(name)) else { return false };
        let built = (p.build)();
        if replace {
            self.graph = built;
            self.set_status(format!("Replaced the graph with preset “{}”", p.name));
        } else {
            let added = self.graph.merge_preset(&built, p.name);
            self.nodes.multi = added.clone();
            self.set_status(format!("Added preset “{}” to the graph ({} new nodes, grouped)", p.name, added.len()));
        }
        self.nodes.selected = None;
        self.nodes.fit_pending = true;
        self.nodes.touched = false;
        true
    }
}
