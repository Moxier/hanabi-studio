//! Simple Effects view: a scene look, an on/off stack of effects with sliders, and per-model /
//! per-material look assignment. It edits the same graph the node editor shows.

use crate::app::App;
use crate::effects::{presets, EffectGraph, ModelLook, Node, NodeKind, PKind, PVal, Socket};
use crate::ui::theme;
use egui::{Color32, RichText};

/// Sliders, colours, choices and toggles for one effect. Returns true when something changed.
pub fn node_params(ui: &mut egui::Ui, node: &mut Node) -> bool {
    let info = node.kind.info();
    let mut changed = false;
    for (pi, spec) in info.params.iter().enumerate() {
        let Some(v) = node.values.get_mut(pi) else { continue };
        ui.horizontal(|ui| match (&spec.kind, v) {
            (PKind::Float(min, max, _), PVal::F(x)) => {
                changed |= ui.add(egui::Slider::new(x, *min..=*max).text(spec.name)).changed();
            }
            (PKind::Color(_), PVal::C(c)) => {
                changed |= ui.color_edit_button_rgb(c).changed();
                ui.label(spec.name);
            }
            (PKind::Choice(opts, _), PVal::I(i)) => {
                egui::ComboBox::from_id_salt((node.id, pi)).selected_text(opts.get(*i).copied().unwrap_or("?")).show_ui(ui, |ui| {
                    for (k, o) in opts.iter().enumerate() {
                        changed |= ui.selectable_value(i, k, *o).changed();
                    }
                });
                ui.label(spec.name);
            }
            (PKind::Toggle(_), PVal::B(b)) => {
                changed |= ui.checkbox(b, spec.name).changed();
            }
            _ => {}
        })
        .response
        .on_hover_text(spec.help);
    }
    if info.params.is_empty() {
        ui.label(RichText::new("No settings.").color(theme::TEXT_DIM));
    }
    changed
}

#[derive(Clone, Copy, PartialEq)]
enum SceneLook {
    Classic,
    Polished,
    Bright,
    Realistic,
}

impl App {
    pub fn effects_ws(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        use crate::app::FxTab;
        if !self.fx_simple {
            self.fx_simple = true;
        }
        egui::Panel::left("fx_side").resizable(true).default_size(360.0).size_range(290.0..=560.0).show(ui, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                for (t, l, tip) in [
                    (FxTab::Lights, "💡 Lights", "Lighting style, sky and light sources"),
                    (FxTab::Materials, "🎨 Materials", "What each part of a model is made of"),
                    (FxTab::Stage, "✨ Stage", "Particles, beams, water, mirror floor"),
                    (FxTab::Screen, "🎥 Camera & Screen", "Depth of field, bloom, colour and other effects on the picture"),
                ] {
                    let sel = self.fx_tab == t;
                    let b = egui::Button::new(RichText::new(l).size(12.5).color(if sel { Color32::WHITE } else { theme::TEXT_DIM }))
                        .fill(if sel { theme::ACCENT.linear_multiply(0.4) } else { theme::BG2 });
                    if ui.add(b).on_hover_text(tip).clicked() {
                        self.fx_tab = t;
                    }
                }
            });
            ui.separator();
            egui::ScrollArea::vertical().show(ui, |ui| match self.fx_tab {
                FxTab::Lights => self.lights_section(ui),
                FxTab::Materials => self.materials_list(ui),
                FxTab::Stage => self.fx_section(ui, FxTab::Stage),
                FxTab::Screen => self.fx_section(ui, FxTab::Screen),
            });
        });
        if self.fx_tab == FxTab::Materials {
            egui::Panel::right("mat_editor").resizable(true).default_size(340.0).size_range(280.0..=520.0).show(ui, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| self.material_editor(ui));
            });
            if self.mat_nodes_open && self.mat_sel.is_some() {
                egui::Panel::bottom("mat_nodes").resizable(true).default_size(330.0).size_range(180.0..=800.0).frame(egui::Frame::new().fill(theme::BG0)).show(ui, |ui| {
                    self.mat_node_canvas(ui);
                });
            }
        }
        egui::CentralPanel::default().frame(egui::Frame::new().fill(theme::BG0)).show(ui, |ui| {
            let rect = ui.max_rect();
            let bar = egui::Rect::from_min_size(rect.min, egui::vec2(rect.width(), 30.0));
            ui.scope_builder(egui::UiBuilder::new().max_rect(bar.shrink2(egui::vec2(8.0, 3.0))), |ui| {
                ui.horizontal_centered(|ui| self.fx_view_switch(ui));
            });
            let vr = egui::Rect::from_min_max(rect.min + egui::vec2(0.0, 30.0), rect.max);
            self.viewport(ui, frame, vr, false);
            if self.fx_tab == FxTab::Lights {
                self.light_overlay(ui);
            }
        });
    }

    pub fn fx_view_switch(&mut self, ui: &mut egui::Ui) {
        ui.selectable_value(&mut self.fx_simple, true, "Simple panels");
        if ui.button("🔗 Project graph").on_hover_text("See and edit the whole project as nodes (F5)").clicked() {
            self.ws = crate::app::Workspace::Graph;
            self.fx_simple = true;
        }
        ui.separator();
        ui.label(RichText::new(format!("Look: {}", self.graph.preset_name)).size(11.5).color(theme::TEXT_DIM));
    }

    pub fn apply_fx_actions(&mut self, actions: Vec<FxAction>, chain: &[u32]) {
        for a in actions {
            match a {
                FxAction::Remove(id) => {
                    self.graph.remove(id);
                    self.graph.preset_name = "Custom".into();
                }
                FxAction::Up(i) => {
                    let mut c = chain.to_vec();
                    c.swap(i, i - 1);
                    self.graph.set_chain(&c);
                    self.nodes.fit_pending = true;
                }
                FxAction::Down(i) => {
                    let mut c = chain.to_vec();
                    c.swap(i, i + 1);
                    self.graph.set_chain(&c);
                    self.nodes.fit_pending = true;
                }
                FxAction::Edited => {
                    self.graph.preset_name = format!("{} (edited)", self.graph.preset_name.trim_end_matches(" (edited)"));
                }
            }
        }
    }

    /// The scene-wide lighting style buttons and their fine-tuning rows (used by the Lights tab).
    pub fn lighting_style(&mut self, ui: &mut egui::Ui) {
        theme::section(ui, "Lighting style");
        let cur = self.scene_look();
        ui.horizontal_wrapped(|ui| {
            for (l, name, tip) in [
                (SceneLook::Classic, "Classic toon", "Flat toon shading with outlines"),
                (SceneLook::Polished, "Polished toon", "Smooth anime shading with rim light"),
                (SceneLook::Bright, "Bright anime", "High-key, lifted shadows"),
                (SceneLook::Realistic, "Realistic", "Ray Lighting: physically based light and materials"),
            ] {
                if ui.selectable_label(cur == l, name).on_hover_text(tip).clicked() && cur != l {
                    self.set_scene_look(l);
                }
            }
        });
        let shading: Vec<u32> = self
            .graph
            .fx_ids()
            .into_iter()
            .filter(|id| matches!(self.graph.node(*id).map(|n| n.kind), Some(NodeKind::ShadingStyle) | Some(NodeKind::RayLighting) | Some(NodeKind::Sky)))
            .collect();
        let mut actions = Vec::new();
        for id in &shading {
            if let Some(a) = self.fx_row(ui, *id, None) {
                actions.push(a);
            }
            ui.add_space(2.0);
        }
        if self.graph.fx_of(NodeKind::Sky).is_none() && ui.button("＋ Sky gradient (background and sky light colour)").clicked() {
            self.graph.append_before_output(NodeKind::Sky);
        }
        self.apply_fx_actions(actions, &[]);
    }

    fn scene_look(&self) -> SceneLook {
        if self.graph.fx_of(NodeKind::RayLighting).is_some() {
            return SceneLook::Realistic;
        }
        match self.graph.fx_of(NodeKind::ShadingStyle).map(|n| n.i("Style")) {
            Some(1) => SceneLook::Polished,
            Some(3) => SceneLook::Bright,
            Some(2) => SceneLook::Realistic,
            _ => SceneLook::Classic,
        }
    }

    fn set_scene_look(&mut self, look: SceneLook) {
        let g = &mut self.graph;
        let ray_ids: Vec<u32> = g.nodes.iter().filter(|n| n.kind == NodeKind::RayLighting).map(|n| n.id).collect();
        if look == SceneLook::Realistic {
            if ray_ids.is_empty() {
                g.append_before_output(NodeKind::RayLighting);
            }
            for n in g.nodes.iter_mut().filter(|n| n.kind == NodeKind::RayLighting) {
                n.enabled = true;
            }
        } else {
            for id in ray_ids {
                g.remove(id);
            }
            let style = match look {
                SceneLook::Polished => 1,
                SceneLook::Bright => 3,
                _ => 0,
            };
            let sid = match g.fx_ids().into_iter().find(|id| g.node(*id).map(|n| n.kind) == Some(NodeKind::ShadingStyle)) {
                Some(id) => id,
                None => g.append_before_output(NodeKind::ShadingStyle),
            };
            if let Some(n) = g.node_mut(sid) {
                n.enabled = true;
                n.set("Style", PVal::I(style));
            }
        }
        g.preset_name = "Custom".into();
    }

    fn fx_row(&mut self, ui: &mut egui::Ui, id: u32, pos: Option<(usize, usize)>) -> Option<FxAction> {
        let mut action = None;
        let Some(node) = self.graph.node_mut(id) else { return None };
        let info = node.kind.info();
        let open_id = ui.make_persistent_id(("fxrow", id));
        let mut state = egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), open_id, false);
        egui::Frame::new()
            .fill(if node.enabled { theme::BG2 } else { theme::BG1 })
            .stroke(egui::Stroke::new(1.0, theme::BG3))
            .corner_radius(egui::CornerRadius::same(6))
            .inner_margin(egui::Margin::symmetric(6, 4))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let arrow = if state.is_open() { "▼" } else { "▶" };
                    if ui.add(egui::Button::new(RichText::new(arrow).size(10.0)).frame(false)).clicked() {
                        state.toggle(ui);
                    }
                    ui.checkbox(&mut node.enabled, "");
                    let title = RichText::new(info.title).strong().color(if node.enabled { Color32::WHITE } else { theme::TEXT_DIM });
                    if ui.add(egui::Label::new(title).sense(egui::Sense::click())).on_hover_text(info.about).clicked() {
                        state.toggle(ui);
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.add(egui::Button::new("×").frame(false)).on_hover_text("Remove").clicked() {
                            action = Some(FxAction::Remove(id));
                        }
                        if let Some((i, n)) = pos {
                            if ui.add_enabled(i + 1 < n, egui::Button::new("▼").small()).on_hover_text("Later in the chain").clicked() {
                                action = Some(FxAction::Down(i));
                            }
                            if ui.add_enabled(i > 0, egui::Button::new("▲").small()).on_hover_text("Earlier in the chain").clicked() {
                                action = Some(FxAction::Up(i));
                            }
                        }
                    });
                });
                state.show_body_unindented(ui, |ui| {
                    ui.label(RichText::new(info.about).size(10.5).color(theme::TEXT_DIM));
                    if node_params(ui, node) && action.is_none() {
                        action = Some(FxAction::Edited);
                    }
                });
            });
        state.store(ui.ctx());
        action
    }

    /// Stage effects or the screen-effect chain, with add menus and presets.
    pub fn fx_section(&mut self, ui: &mut egui::Ui, tab: crate::app::FxTab) {
        use crate::app::FxTab;
        let stage_kinds = [NodeKind::Particles, NodeKind::LightBeams, NodeKind::Water, NodeKind::MirrorFloor];
        let mut actions = Vec::new();
        let chain = self.graph.chain_ids();
        if tab == FxTab::Stage {
            theme::section(ui, "Stage & atmosphere");
            ui.horizontal_wrapped(|ui| {
                for k in stage_kinds {
                    let info = k.info();
                    if ui.button(format!("＋ {}", info.title)).on_hover_text(info.about).clicked() {
                        self.graph.append_before_output(k);
                        self.graph.preset_name = "Custom".into();
                    }
                }
            });
            let fx: Vec<u32> = self
                .graph
                .fx_ids()
                .into_iter()
                .filter(|id| !matches!(self.graph.node(*id).map(|n| n.kind), Some(NodeKind::ShadingStyle) | Some(NodeKind::RayLighting) | Some(NodeKind::Sky)))
                .collect();
            if fx.is_empty() {
                ui.label(RichText::new("Nothing on stage yet — add particles, light beams, water or a mirror floor above.").size(11.0).color(theme::TEXT_DIM));
            }
            for id in &fx {
                if let Some(a) = self.fx_row(ui, *id, None) {
                    actions.push(a);
                }
                ui.add_space(2.0);
            }
        } else {
            theme::section(ui, "Camera & screen effects (top runs first)");
            ui.horizontal(|ui| {
                ui.menu_button("＋ Add effect", |ui| {
                    let mut cats: Vec<&str> = Vec::new();
                    for k in NodeKind::ALL.iter().filter(|k| k.info().output == Some(Socket::Image) && **k != NodeKind::SceneRender) {
                        let c = k.info().category;
                        if !cats.contains(&c) {
                            cats.push(c);
                        }
                    }
                    for c in cats {
                        ui.menu_button(c, |ui| {
                            for k in NodeKind::ALL.iter().filter(|k| k.info().category == c && k.info().output == Some(Socket::Image) && **k != NodeKind::SceneRender) {
                                let info = k.info();
                                if ui.button(info.title).on_hover_text(info.about).clicked() {
                                    self.graph.append_before_output(*k);
                                    self.graph.preset_name = "Custom".into();
                                    ui.close();
                                }
                            }
                        });
                    }
                });
                ui.menu_button("Presets…", |ui| {
                    let mut pick = None;
                    ui.label(RichText::new("Built-in looks (add)").strong());
                    for p in presets() {
                        if ui.button(p.name).on_hover_text(p.about).clicked() {
                            pick = Some((p.name.to_string(), false, false));
                            ui.close();
                        }
                    }
                    if !self.user_presets.screen_stacks.is_empty() {
                        ui.separator();
                        ui.label(RichText::new("My presets (add)").strong());
                        for (n, _) in &self.user_presets.screen_stacks {
                            if ui.button(n).clicked() {
                                pick = Some((n.clone(), false, true));
                                ui.close();
                            }
                        }
                    }
                    ui.separator();
                    ui.menu_button("Replace everything with…", |ui| {
                        for p in presets() {
                            if ui.button(p.name).on_hover_text(p.about).clicked() {
                                pick = Some((p.name.to_string(), true, false));
                                ui.close();
                            }
                        }
                    });
                    if let Some((n, r, user)) = pick {
                        if user {
                            if let Some((_, g)) = self.user_presets.screen_stacks.iter().find(|(x, _)| *x == n).cloned() {
                                let added = self.graph.merge_preset(&g, &n);
                                self.set_status(format!("Added your preset “{n}” ({} effects)", added.len()));
                            }
                        } else {
                            if r {
                                self.ask(
                                    "Replace all effects?",
                                    &format!("Every effect, look and stage item will be replaced by the “{n}” preset."),
                                    "Replace",
                                    crate::ui::home::ConfirmAction::ReplaceGraph(n.clone()),
                                );
                            } else {
                                self.apply_preset(&n, false);
                            }
                        }
                    }
                });
                if ui.button("Clear").on_hover_text("Remove all screen effects").clicked() && !chain.is_empty() {
                    self.ask("Clear screen effects?", &format!("All {} screen effects will be removed.", chain.len()), "Clear", crate::ui::home::ConfirmAction::ClearScreenFx);
                }
            });
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut self.preset_name_buf).hint_text("preset name").desired_width(150.0));
                if ui.button("💾 Save as my preset").on_hover_text("Reuse these screen effects in any project").clicked() {
                    let name = if self.preset_name_buf.trim().is_empty() { format!("My look {}", self.user_presets.screen_stacks.len() + 1) } else { self.preset_name_buf.trim().to_string() };
                    // Keep only the screen chain (scene looks belong to Lights / Stage).
                    let mut g = EffectGraph::empty();
                    let ids: Vec<u32> = chain.clone();
                    let mut new_ids = Vec::new();
                    for id in &ids {
                        if let Some(n) = self.graph.node(*id) {
                            let nid = g.add(n.kind, n.pos);
                            if let Some(x) = g.node_mut(nid) {
                                x.values = n.values.clone();
                                x.enabled = n.enabled;
                            }
                            new_ids.push(nid);
                        }
                    }
                    g.set_chain(&new_ids);
                    g.preset_name = name.clone();
                    self.user_presets.screen_stacks.retain(|(n, _)| *n != name);
                    self.user_presets.screen_stacks.push((name.clone(), g));
                    match self.user_presets.save() {
                        Ok(_) => self.set_status(format!("Saved preset “{name}” — it's available in every project")),
                        Err(e) => self.set_error(format!("Could not save preset: {e}")),
                    }
                }
            });
            if chain.is_empty() {
                ui.label(RichText::new("None — add depth of field, bloom, tone map, reflections… with ＋ Add effect.").size(11.0).color(theme::TEXT_DIM));
            }
            let n = chain.len();
            for (i, id) in chain.iter().enumerate() {
                if let Some(a) = self.fx_row(ui, *id, Some((i, n))) {
                    actions.push(a);
                }
                ui.add_space(2.0);
            }
            let side = self.graph.nodes.iter().filter(|nd| nd.kind != NodeKind::SceneRender && nd.kind.info().output == Some(Socket::Image) && !chain.contains(&nd.id)).count();
            if side > 0 {
                ui.label(RichText::new(format!("{side} more effect(s) sit on side branches — open the node graph to see them.")).size(10.5).color(theme::KEY));
            }
        }
        self.apply_fx_actions(actions, &chain);
        ui.add_space(8.0);
        if let Some(r) = &self.renderer {
            ui.label(RichText::new(format!("{:.2} ms per frame · {} draw calls · {} particles", r.stats.gpu_ms, r.stats.draw_calls, r.stats.particles)).size(10.5).color(theme::TEXT_DIM));
        }
        ui.label(RichText::new("Built-in GPU effects only; MME effect files are not supported.").size(10.5).color(theme::TEXT_DIM));
    }

    /// Per-model look and per-material type, the way effects are assigned to objects.
    pub fn assign_looks(&mut self, ui: &mut egui::Ui) {
        theme::section(ui, "Assign looks");
        ui.label(RichText::new("Give each model its own look. Materials say what each part is made of (used by the Realistic look).").size(11.0).color(theme::TEXT_DIM));
        let names: Vec<String> = self.scene.models.iter().filter(|m| !m.is_accessory()).map(|m| m.name().to_string()).collect();
        if names.is_empty() {
            ui.label(RichText::new("Load a model to assign looks.").color(theme::TEXT_DIM));
        }
        for name in &names {
            theme::card(ui, |ui| {
                ui.label(RichText::new(name).strong());
                let cur = self.graph.model_looks.get(name).copied();
                let text = cur.map(|l| l.label()).unwrap_or("Same as scene");
                egui::ComboBox::from_id_salt(("look", name)).width(200.0).selected_text(text).show_ui(ui, |ui| {
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
            ui.add_space(3.0);
        }
        ui.add_space(4.0);
        self.material_types(ui);
    }
}

pub enum FxAction {
    Remove(u32),
    Up(usize),
    Down(usize),
    Edited,
}
