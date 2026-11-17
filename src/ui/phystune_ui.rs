//! Physics panel. "Global": live settings for the whole scene (gravity, time step, sub-steps,
//! damping and spring multipliers, wind, stability) and display switches. "Parts": every rigid
//! body and joint of a model, each editable; changes rebuild that model's simulation, reset it
//! to the animation pose, and are saved to the model's preset file (the model file is never
//! changed).

use crate::app::App;
use crate::model::{RigidMode, Shape};
use crate::physics::BigMove;
use crate::ui::theme;
use egui::{vec2, Color32, RichText};
use glam::Vec3;

fn vec3_edit(ui: &mut egui::Ui, v: &mut Vec3, speed: f64, deg: bool) -> bool {
    let mut changed = false;
    for (k, label) in ["x", "y", "z"].iter().enumerate() {
        let mut x = if deg { v[k].to_degrees() } else { v[k] };
        let r = ui.add(egui::DragValue::new(&mut x).speed(speed).prefix(format!("{label} ")).max_decimals(3));
        if r.changed() {
            v[k] = if deg { x.to_radians() } else { x };
            changed = true;
        }
    }
    changed
}

fn mode_name(m: RigidMode) -> &'static str {
    match m {
        RigidMode::FollowBone => "Follow bone",
        RigidMode::Physics => "Physics",
        RigidMode::PhysicsWithBonePosition => "Physics + bone position",
    }
}

fn mode_color(m: RigidMode) -> Color32 {
    match m {
        RigidMode::FollowBone => Color32::from_rgb(90, 200, 255),
        RigidMode::Physics => Color32::from_rgb(255, 170, 60),
        RigidMode::PhysicsWithBonePosition => Color32::from_rgb(140, 230, 120),
    }
}

/// Everything the Physics window can change, for undo / redo.
#[derive(Clone, PartialEq)]
pub struct PhysState {
    pub label: String,
    cfg: crate::physics::PhysicsCfg,
    substeps: u32,
    physics_on: bool,
    models: Vec<(u64, bool, crate::phystune::PhysicsTweaks)>,
}

impl PhysState {
    fn same(&self, o: &PhysState) -> bool {
        self.cfg == o.cfg && self.substeps == o.substeps && self.physics_on == o.physics_on && self.models == o.models
    }
}

impl App {
    pub fn phys_snapshot(&self, label: &str) -> PhysState {
        PhysState {
            label: label.to_string(),
            cfg: self.scene.physics_cfg.clone(),
            substeps: self.scene.stage.physics_substeps,
            physics_on: self.scene.stage.physics,
            models: self.scene.models.iter().map(|m| (m.id, m.physics_enabled, m.phys_tweaks.clone())).collect(),
        }
    }

    /// Put a saved physics state back: settings, per-model on/off and part tweaks (models whose
    /// tweaks changed are rebuilt and their presets saved), then reset to the animation pose.
    fn phys_restore(&mut self, st: &PhysState) {
        self.scene.physics_cfg = st.cfg.clone();
        self.scene.stage.physics_substeps = st.substeps;
        self.scene.stage.physics = st.physics_on;
        for (id, on, tw) in &st.models {
            if let Some(i) = self.scene.model_by_id(*id) {
                let m = &mut self.scene.models[i];
                m.physics_enabled = *on;
                if &m.phys_tweaks != tw {
                    m.phys_tweaks = tw.clone();
                    m.rebuild_physics();
                    if let Some(k) = &m.file_key {
                        let _ = crate::phystune::save(k, &m.phys_tweaks);
                    }
                }
                m.physics_frozen = false;
            }
        }
        self.phys_pending = None;
        self.phys_warning = None;
        self.scene.force_reset_physics();
    }

    /// Record an undo step (for changes made outside the Physics window, e.g. by the AI).
    pub fn phys_push_undo(&mut self, label: &str) {
        let st = self.phys_snapshot(label);
        self.phys_undo.push(st);
        if self.phys_undo.len() > 100 {
            self.phys_undo.remove(0);
        }
        self.phys_redo.clear();
    }

    pub fn phys_undo(&mut self) {
        let Some(prev) = self.phys_undo.pop() else {
            self.set_status("Nothing to undo in physics");
            return;
        };
        let cur = self.phys_snapshot(&prev.label);
        self.phys_redo.push(cur);
        self.phys_restore(&prev);
        self.set_status(format!("Undid: {}", prev.label));
    }

    pub fn phys_redo(&mut self) {
        let Some(next) = self.phys_redo.pop() else { return };
        let cur = self.phys_snapshot(&next.label);
        self.phys_undo.push(cur);
        self.phys_restore(&next);
        self.set_status(format!("Redid: {}", next.label));
    }

    /// Whether the mouse is over the Physics window (Ctrl+Z then undoes physics edits).
    pub fn phys_hovered(&self, ctx: &egui::Context) -> bool {
        self.show_physics && ctx.pointer_hover_pos().map(|p| self.phys_win_rect.contains(p)).unwrap_or(false)
    }

    pub fn physics_window(&mut self, ctx: &egui::Context) {
        self.physics_apply_pending(ctx);
        if !self.show_physics {
            return;
        }
        let before = self.phys_snapshot("");
        let mut action: Option<bool> = None; // Some(true) undo, Some(false) redo
        let mut open = true;
        let resp = egui::Window::new("Physics").open(&mut open).default_width(440.0).default_height(620.0).resizable(true).show(ctx, |ui| {
            // Undo / redo for everything in this window
            ui.horizontal(|ui| {
                let ul = self.phys_undo.last().map(|s| format!("Undo: {} (Ctrl+Z)", s.label)).unwrap_or_else(|| "Nothing to undo".into());
                if ui.add_enabled(!self.phys_undo.is_empty(), egui::Button::new("⟲ Undo")).on_hover_text(ul).clicked() {
                    action = Some(true);
                }
                let rl = self.phys_redo.last().map(|s| format!("Redo: {} (Ctrl+Y)", s.label)).unwrap_or_else(|| "Nothing to redo".into());
                if ui.add_enabled(!self.phys_redo.is_empty(), egui::Button::new("⟳ Redo")).on_hover_text(rl).clicked() {
                    action = Some(false);
                }
                ui.label(RichText::new(format!("{} step(s)", self.phys_undo.len())).size(10.5).color(theme::TEXT_DIM));
            });
            // Warnings with a way back
            let blown: u32 = self.scene.models.iter().filter_map(|m| m.physics.as_ref().map(|p| p.blowups)).sum();
            if blown > self.phys_blowups_seen || self.phys_warning.is_some() {
                egui::Frame::new().fill(Color32::from_rgb(70, 40, 30)).stroke(egui::Stroke::new(1.0, Color32::from_rgb(255, 150, 90))).corner_radius(egui::CornerRadius::same(6)).inner_margin(egui::Margin::same(8)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    let text = match &self.phys_warning {
                        Some((_, w)) => w.clone(),
                        None => format!("Parts flew off and were put back on their bones ({} time(s)). If this started after a change, undo it.", blown - self.phys_blowups_seen),
                    };
                    ui.label(RichText::new(format!("⚠ {text}")).color(Color32::from_rgb(255, 210, 170)));
                    ui.horizontal(|ui| {
                        if ui.add_enabled(!self.phys_undo.is_empty(), egui::Button::new("⟲ Undo last change")).clicked() {
                            action = Some(true);
                        }
                        if ui.button("⟲ Reset to animation pose").clicked() {
                            self.scene.force_reset_physics();
                        }
                        if ui.button("Dismiss").clicked() {
                            self.phys_warning = None;
                            self.phys_blowups_seen = blown;
                        }
                    });
                });
            }
            ui.horizontal(|ui| {
                ui.checkbox(&mut self.scene.stage.physics, "Simulate physics");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("⟲ Reset to animation pose").on_hover_text("Put every hair, skirt and prop part back on its bone at the current frame, then let it settle").clicked() {
                        self.scene.force_reset_physics();
                    }
                });
            });
            ui.horizontal(|ui| {
                ui.checkbox(&mut self.scene.stage.show_rigid, "Show rigid bodies");
                ui.checkbox(&mut self.scene.stage.show_joints, "Show joints");
                ui.checkbox(&mut self.scene.stage.show_physics_bones, "Physics bones");
            });
            ui.horizontal(|ui| {
                for (t, label) in [(0u8, "Global"), (1u8, "Parts")] {
                    if ui.selectable_label(self.phys_tab == t, RichText::new(label).size(13.0)).clicked() {
                        self.phys_tab = t;
                    }
                }
            });
            ui.separator();
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                if self.phys_tab == 0 {
                    self.physics_global_ui(ui);
                } else {
                    self.physics_parts_ui(ui);
                }
            });
        });
        if let Some(r) = resp {
            self.phys_win_rect = r.response.rect;
        }
        self.show_physics = open;
        let after = self.phys_snapshot("");
        if !after.same(&before) {
            if !self.phys_edit_open {
                let label = if after.cfg != before.cfg || after.substeps != before.substeps || after.physics_on != before.physics_on {
                    "global physics setting".to_string()
                } else {
                    let changed: Vec<String> = after
                        .models
                        .iter()
                        .zip(&before.models)
                        .filter(|(a, b)| a != b)
                        .filter_map(|(a, _)| self.scene.model_by_id(a.0).map(|i| self.scene.models[i].data.name.clone()))
                        .collect();
                    format!("physics of {}", changed.join(", "))
                };
                let mut b = before;
                b.label = label;
                self.phys_undo.push(b);
                if self.phys_undo.len() > 100 {
                    self.phys_undo.remove(0);
                }
                self.phys_redo.clear();
            }
            self.phys_edit_open = true;
        }
        if !ctx.input(|i| i.pointer.any_down()) {
            self.phys_edit_open = false;
        }
        match action {
            Some(true) => self.phys_undo(),
            Some(false) => self.phys_redo(),
            None => {}
        }
    }

    fn physics_global_ui(&mut self, ui: &mut egui::Ui) {
        let dflt = crate::physics::PhysicsCfg::default();
        let c = &mut self.scene.physics_cfg;
        theme::section(ui, "Simulation (live)");
        theme::dbl(ui.add(egui::Slider::new(&mut c.gravity, 0.0..=300.0).text("Gravity")), &mut c.gravity, dflt.gravity);
        ui.horizontal(|ui| {
            ui.label("Gravity direction");
            for v in c.gravity_dir.iter_mut() {
                ui.add(egui::DragValue::new(v).speed(0.02).range(-1.0..=1.0));
            }
            if ui.small_button("Down").clicked() {
                c.gravity_dir = [0.0, -1.0, 0.0];
            }
        });
        ui.horizontal(|ui| {
            ui.label("Time step");
            let label = |v: f32| if v <= 0.0 { "Follow frame rate".to_string() } else { format!("Fixed 1/{:.0} s", 1.0 / v) };
            egui::ComboBox::from_id_salt("phys_step").selected_text(label(c.fixed_step)).show_ui(ui, |ui| {
                for v in [0.0, 1.0 / 60.0, 1.0 / 120.0, 1.0 / 240.0] {
                    ui.selectable_value(&mut c.fixed_step, v, label(v));
                }
            })
            .response
            .on_hover_text("Follow frame rate: each frame's time is split into sub-steps.\nFixed: the simulation always advances in equal steps, whatever the frame rate.");
        });
        let follow = c.fixed_step <= 0.0;
        ui.add_enabled(follow, egui::Slider::new(&mut self.scene.stage.physics_substeps, 1..=8).text("Sub-steps per frame"))
            .on_disabled_hover_text("Used with “Follow frame rate”");
        let c = &mut self.scene.physics_cfg;
        theme::dbl(ui.add(egui::Slider::new(&mut c.damping_mul, 0.0..=3.0).text("Damping ×").step_by(0.05)).on_hover_text("Multiplies every part's damping: higher = calmer, less swinging"), &mut c.damping_mul, 1.0);
        theme::dbl(ui.add(egui::Slider::new(&mut c.spring_mul, 0.0..=4.0).text("Spring stiffness ×").step_by(0.05)).on_hover_text("Multiplies every joint spring: higher = parts spring back to shape faster"), &mut c.spring_mul, 1.0);
        theme::section(ui, "Wind");
        theme::dbl(ui.add(egui::Slider::new(&mut c.wind_strength, 0.0..=200.0).text("Strength")), &mut c.wind_strength, 0.0);
        theme::dbl(ui.add(egui::Slider::new(&mut c.wind_dir, -180.0..=180.0).text("Direction (°)")), &mut c.wind_dir, 0.0);
        theme::dbl(ui.add(egui::Slider::new(&mut c.wind_gusts, 0.0..=1.0).text("Gusts (noise)")), &mut c.wind_gusts, dflt.wind_gusts);
        theme::dbl(ui.add(egui::Slider::new(&mut c.air_drag, 0.0..=1.0).text("Air drag")), &mut c.air_drag, 0.0);
        theme::section(ui, "Stability");
        theme::dbl(ui.add(egui::Slider::new(&mut c.max_speed, 20.0..=1000.0).logarithmic(true).text("Speed limit")), &mut c.max_speed, dflt.max_speed);
        ui.horizontal(|ui| {
            ui.label("After a sudden big move");
            egui::ComboBox::from_id_salt("bigmove")
                .selected_text(match c.big_move {
                    BigMove::Snap => "Snap back",
                    BigMove::Soften => "Carry along",
                    BigMove::Off => "Do nothing",
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut c.big_move, BigMove::Snap, "Snap back").on_hover_text("Put hair and cloth straight back on their bones");
                    ui.selectable_value(&mut c.big_move, BigMove::Soften, "Carry along").on_hover_text("Move them with the character, keeping a little swing");
                    ui.selectable_value(&mut c.big_move, BigMove::Off, "Do nothing");
                });
        });
        theme::dbl(ui.add(egui::Slider::new(&mut c.big_move_dist, 0.5..=30.0).text("Big move = more than (units/frame)")), &mut c.big_move_dist, dflt.big_move_dist);
        theme::dbl(ui.add(egui::Slider::new(&mut c.settle_steps, 0..=200).text("Settle steps after a jump")), &mut c.settle_steps, dflt.settle_steps);
        ui.horizontal(|ui| {
            if ui.button("Defaults").on_hover_text("Scene physics settings back to their defaults (part tweaks stay)").clicked() {
                self.scene.physics_cfg = Default::default();
                self.scene.stage.physics_substeps = 2;
            }
        });
        let snaps: u32 = self.scene.models.iter().filter_map(|m| m.physics.as_ref().map(|p| p.snaps)).sum();
        ui.label(RichText::new(format!("Sudden moves caught so far: {snaps}")).size(10.5).color(theme::TEXT_DIM));
        theme::section(ui, "Per model");
        for m in &mut self.scene.models {
            if m.physics.is_some() {
                let failed = m.physics.as_ref().map(|p| p.failed).unwrap_or(false);
                ui.horizontal(|ui| {
                    ui.checkbox(&mut m.physics_enabled, m.data.name.clone());
                    if !m.phys_tweaks.is_empty() {
                        ui.label(RichText::new(format!("{} tweaked parts", m.phys_tweaks.bodies.len() + m.phys_tweaks.joints.len())).size(10.5).color(theme::KEY));
                    }
                    if failed {
                        ui.label(RichText::new("physics stopped (bad data)").size(10.5).color(Color32::from_rgb(255, 130, 120)));
                    }
                });
            }
        }
    }

    fn physics_parts_ui(&mut self, ui: &mut egui::Ui) {
        let with_phys: Vec<(u64, String)> = self.scene.models.iter().filter(|m| m.physics.is_some() || !m.data.rigid_bodies.is_empty()).map(|m| (m.id, m.data.name.clone())).collect();
        if with_phys.is_empty() {
            ui.label(RichText::new("No model with physics in the scene.").color(theme::TEXT_DIM));
            return;
        }
        // Which model
        let cur_id = self
            .phys_sel
            .map(|s| s.0)
            .filter(|id| with_phys.iter().any(|(i, _)| i == id))
            .or_else(|| self.scene.selected.and_then(|i| self.scene.models.get(i)).map(|m| m.id).filter(|id| with_phys.iter().any(|(i, _)| i == id)))
            .unwrap_or(with_phys[0].0);
        let Some(mi) = self.scene.model_by_id(cur_id) else { return };
        ui.horizontal(|ui| {
            ui.label("Model");
            egui::ComboBox::from_id_salt("phys_model").selected_text(self.scene.models[mi].data.name.clone()).show_ui(ui, |ui| {
                for (id, name) in &with_phys {
                    if ui.selectable_label(*id == cur_id, name).clicked() {
                        self.phys_sel = Some((*id, false, usize::MAX));
                    }
                }
            });
        });
        // Preset status
        let (n_tweaks, key) = {
            let m = &self.scene.models[mi];
            (m.phys_tweaks.bodies.len() + m.phys_tweaks.joints.len(), m.file_key.clone())
        };
        theme::card(ui, |ui| {
            ui.set_width(ui.available_width());
            if n_tweaks == 0 {
                ui.label(RichText::new("Using the model's own physics. Changes you make here are saved for this model in its own preset file; the model file is never changed.").size(11.0).color(theme::TEXT_DIM));
            } else {
                ui.label(RichText::new(format!("{n_tweaks} part(s) changed · saved automatically for this model")).size(11.5).color(theme::KEY));
                ui.horizontal(|ui| {
                    if ui.button("Revert all to the model's values").clicked() {
                        let m = &mut self.scene.models[mi];
                        m.phys_tweaks = Default::default();
                        m.rebuild_physics();
                        if let Some(k) = &key {
                            let _ = crate::phystune::save(k, &m.phys_tweaks);
                        }
                        self.scene.force_reset_physics();
                        self.set_status("Physics back to the model's own values");
                    }
                    if ui.button("Open preset folder").clicked() {
                        let d = crate::phystune::preset_dir();
                        let _ = std::fs::create_dir_all(&d);
                        crate::ui::links::open_external(d.as_os_str());
                    }
                });
            }
            if key.is_none() {
                ui.label(RichText::new("This model wasn't loaded from a file, so tweaks can't be saved.").size(10.5).color(theme::TEXT_DIM));
            }
        });
        // List
        ui.horizontal(|ui| {
            if ui.selectable_label(!self.phys_list_joints, format!("Rigid bodies ({})", self.scene.models[mi].data.rigid_bodies.len())).clicked() {
                self.phys_list_joints = false;
            }
            if ui.selectable_label(self.phys_list_joints, format!("Joints ({})", self.scene.models[mi].data.joints.len())).clicked() {
                self.phys_list_joints = true;
            }
            ui.add(egui::TextEdit::singleline(&mut self.phys_search).hint_text("🔍 Find").desired_width(120.0));
        });
        let q = self.phys_search.trim().to_lowercase();
        let joints = self.phys_list_joints;
        let mut pick = None;
        egui::ScrollArea::vertical().id_salt("phys_parts").max_height(200.0).auto_shrink([false, true]).show(ui, |ui| {
            let m = &self.scene.models[mi];
            let d = &m.data;
            if !joints {
                for (i, rb) in d.rigid_bodies.iter().enumerate() {
                    if !q.is_empty() && !rb.name.to_lowercase().contains(&q) {
                        continue;
                    }
                    let sel = self.phys_sel == Some((cur_id, false, i));
                    let tweaked = m.phys_tweaks.bodies.contains_key(&i);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("●").color(mode_color(rb.mode)).size(10.0));
                        if ui.selectable_label(sel, format!("{}{}", rb.name, if tweaked { "  •" } else { "" })).clicked() {
                            pick = Some((cur_id, false, i));
                        }
                    });
                }
            } else {
                for (i, j) in d.joints.iter().enumerate() {
                    if !q.is_empty() && !j.name.to_lowercase().contains(&q) {
                        continue;
                    }
                    let sel = self.phys_sel == Some((cur_id, true, i));
                    let tweaked = m.phys_tweaks.joints.contains_key(&i);
                    if ui.selectable_label(sel, format!("{}{}", j.name, if tweaked { "  •" } else { "" })).clicked() {
                        pick = Some((cur_id, true, i));
                    }
                }
            }
        });
        if let Some(p) = pick {
            self.phys_sel = Some(p);
        }
        ui.label(RichText::new("● physics   ● follows its bone   ● physics + bone position   • changed").size(10.0).color(theme::TEXT_DIM));
        ui.separator();
        match self.phys_sel {
            Some((id, false, i)) if id == cur_id && i < self.scene.models[mi].data.rigid_bodies.len() => self.body_editor(ui, mi, i),
            Some((id, true, i)) if id == cur_id && i < self.scene.models[mi].data.joints.len() => self.joint_editor(ui, mi, i),
            _ => {
                ui.label(RichText::new("Pick a part above to edit it. The selected part is outlined in white in the view.").color(theme::TEXT_DIM));
            }
        }
    }

    /// Store a changed part, rebuild the model's simulation, reset to the animation pose and
    /// save the preset.
    pub fn commit_part(&mut self, mi: usize, body: Option<(usize, crate::model::RigidBody)>, joint: Option<(usize, crate::model::Joint)>) {
        let m = &mut self.scene.models[mi];
        if let Some((i, rb)) = body {
            if m.phys_base.0.get(i) == Some(&rb) {
                m.phys_tweaks.bodies.remove(&i);
            } else {
                m.phys_tweaks.bodies.insert(i, rb);
            }
        }
        if let Some((i, j)) = joint {
            if m.phys_base.1.get(i) == Some(&j) {
                m.phys_tweaks.joints.remove(&i);
            } else {
                m.phys_tweaks.joints.insert(i, j);
            }
        }
        m.phys_tweaks.model_name = m.data.name.clone();
        // While a value is being dragged only the numbers change; the simulation is rebuilt,
        // reset to the animation pose and saved once the mouse button is released.
        m.apply_tweaks_to_data();
        // Hold this model's physics while values are dragged, so it can't fly apart mid-edit.
        m.physics_frozen = true;
        self.phys_pending = Some(m.id);
    }

    /// Finish a part change: rebuild, reset to the animation pose, save the preset.
    pub fn physics_apply_pending(&mut self, ctx: &egui::Context) {
        if self.phys_pending.is_none() || ctx.input(|i| i.pointer.any_down()) {
            return;
        }
        self.physics_finish();
    }

    /// Apply a pending part change now: rebuild, stability check, reset, save the preset.
    /// Returns the warning if the change looks unstable.
    pub fn physics_finish(&mut self) -> Option<String> {
        let Some(id) = self.phys_pending else { return None };
        self.phys_pending = None;
        let Some(mi) = self.scene.model_by_id(id) else { return None };
        let cfg = self.scene.physics_cfg.clone();
        let substeps = self.scene.stage.physics_substeps;
        let m = &mut self.scene.models[mi];
        m.rebuild_physics();
        m.physics_frozen = false;
        // Stability check: simulate the edited physics for a moment and compare with the
        // model's own values. A change that makes parts fly off gets a warning with Undo.
        let (score, broke) = crate::physics::stability_test(&m.data, &m.skel, &cfg, substeps);
        // Compare with the model's own values in the same pose: some poses strain the physics
        // whatever the settings, and that isn't the edit's fault.
        let (base, base_broke) = {
            let mut orig = (*m.data).clone();
            orig.rigid_bodies = m.phys_base.0.clone();
            orig.joints = m.phys_base.1.clone();
            crate::physics::stability_test(&orig, &m.skel, &cfg, substeps)
        };
        m.phys_base_score = Some(base);
        let unstable = if base_broke { false } else { broke || score > (base * 3.0).max(0.35) };
        let saved = match &m.file_key {
            Some(k) => crate::phystune::save(k, &m.phys_tweaks),
            None => Ok(()),
        };
        let id = m.id;
        self.scene.force_reset_physics();
        if unstable {
            self.phys_warning = Some((id, if broke {
                "This change makes the physics blow up (parts fly off). Undo it, or adjust the value again.".to_string()
            } else {
                format!("This change makes the physics unstable (parts drift {:.0}% of the way to flying off, vs {:.0}% before). Undo it if it looks wrong.", score * 100.0, base * 100.0)
            }));
        } else if self.phys_warning.as_ref().map(|w| w.0 == id).unwrap_or(false) {
            self.phys_warning = None;
        }
        match saved {
            Ok(_) if !unstable => self.set_status("Physics updated, checked, reset to the animation pose, and saved for this model"),
            Ok(_) => self.set_error("This physics change looks unstable: see the Physics window"),
            Err(e) => self.set_error(format!("Couldn't save the physics preset: {e}")),
        }
        self.phys_warning.as_ref().filter(|w| w.0 == id).map(|w| w.1.clone())
    }

    fn body_editor(&mut self, ui: &mut egui::Ui, mi: usize, i: usize) {
        let (orig, base, bone_name) = {
            let m = &self.scene.models[mi];
            let rb = m.data.rigid_bodies[i].clone();
            let bone = m.data.bones.get(rb.bone.max(0) as usize).filter(|_| rb.bone >= 0).map(|b| b.name.clone()).unwrap_or_else(|| "no bone".into());
            (rb, m.phys_base.0.get(i).cloned(), bone)
        };
        let mut rb = orig.clone();
        let mut changed = false;
        ui.horizontal(|ui| {
            ui.label(RichText::new(&rb.name).strong().size(14.0));
            ui.label(RichText::new(format!("on bone {bone_name}")).size(10.5).color(theme::TEXT_DIM));
        });
        ui.horizontal(|ui| {
            for mode in [RigidMode::Physics, RigidMode::PhysicsWithBonePosition, RigidMode::FollowBone] {
                if ui.selectable_label(rb.mode == mode, RichText::new(mode_name(mode)).color(mode_color(mode))).clicked() && rb.mode != mode {
                    rb.mode = mode;
                    changed = true;
                }
            }
        });
        theme::section(ui, "Collisions");
        ui.horizontal(|ui| {
            ui.label("Own group");
            let mut g = rb.group as u32 + 1;
            if ui.add(egui::DragValue::new(&mut g).range(1..=16).clamp_existing_to_range(false)).changed() {
                rb.group = (g - 1) as u8;
                changed = true;
            }
            if ui.small_button("Collide with all").clicked() {
                rb.mask = 0xFFFF;
                changed = true;
            }
            if ui.small_button("No collision").on_hover_text("This part goes through everything").clicked() {
                rb.mask = 0;
                changed = true;
            }
        });
        ui.label(RichText::new("Collides with groups (click to switch off: “no collision with X”)").size(10.5).color(theme::TEXT_DIM));
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 3.0;
            for g in 0..16u16 {
                let on = rb.mask & (1 << g) != 0;
                let b = egui::Button::new(RichText::new(format!("{}", g + 1)).size(11.0).color(if on { Color32::WHITE } else { theme::TEXT_DIM }))
                    .fill(if on { theme::ACCENT2.linear_multiply(0.45) } else { theme::BG1 })
                    .min_size(vec2(24.0, 20.0));
                if ui.add(b).on_hover_text(if on { format!("Collides with group {}", g + 1) } else { format!("No collision with group {}", g + 1) }).clicked() {
                    rb.mask ^= 1 << g;
                    changed = true;
                }
            }
        });
        theme::section(ui, "Shape and weight");
        egui::Grid::new(("body_grid", i)).num_columns(2).spacing(vec2(8.0, 4.0)).show(ui, |ui| {
            ui.label("Shape");
            egui::ComboBox::from_id_salt(("shape", i))
                .selected_text(match rb.shape {
                    Shape::Sphere => "Sphere",
                    Shape::Box => "Box",
                    Shape::Capsule => "Capsule",
                })
                .show_ui(ui, |ui| {
                    for (s, n) in [(Shape::Sphere, "Sphere"), (Shape::Box, "Box"), (Shape::Capsule, "Capsule")] {
                        if ui.selectable_label(rb.shape == s, n).clicked() && rb.shape != s {
                            rb.shape = s;
                            changed = true;
                        }
                    }
                });
            ui.end_row();
            ui.label("Size");
            ui.horizontal(|ui| {
                if vec3_edit(ui, &mut rb.size, 0.01, false) {
                    // A zero or negative size breaks the engine (only checked when edited, so
                    // just looking at a part never changes it).
                    rb.size = rb.size.max(Vec3::splat(0.01));
                    changed = true;
                }
            });
            ui.end_row();
            for (label, v, min, max, tip) in [
                ("Mass", &mut rb.mass, 0.001, 100.0, "Heavier parts push lighter ones and swing more slowly"),
                ("Move damping", &mut rb.linear_damping, 0.0, 0.999, "0 = moves freely, 1 = stops almost at once"),
                ("Turn damping", &mut rb.angular_damping, 0.0, 0.999, "0 = swings freely, 1 = barely swings"),
                ("Bounce", &mut rb.restitution, 0.0, 1.0, "How much it bounces off other parts"),
                ("Friction", &mut rb.friction, 0.0, 2.0, "How much it grips when sliding against other parts"),
            ] {
                ui.label(label).on_hover_text(tip);
                if ui.add(egui::DragValue::new(v).speed(0.005).range(min..=max).clamp_existing_to_range(false).max_decimals(3)).changed() {
                    changed = true;
                }
                ui.end_row();
            }
        });
        ui.horizontal(|ui| {
            let tweaked = base.as_ref() != Some(&orig);
            if ui.add_enabled(tweaked, egui::Button::new("Revert this part")).clicked() {
                if let Some(b) = base.clone() {
                    rb = b;
                    changed = true;
                }
            }
            if let Some(b) = &base {
                if tweaked {
                    ui.label(RichText::new(format!("model's value: {} · mass {:.2}", mode_name(b.mode), b.mass)).size(10.5).color(theme::TEXT_DIM));
                }
            }
        });
        if changed {
            self.commit_part(mi, Some((i, rb)), None);
        }
    }

    fn joint_editor(&mut self, ui: &mut egui::Ui, mi: usize, i: usize) {
        let (orig, base, a_name, b_name) = {
            let m = &self.scene.models[mi];
            let j = m.data.joints[i].clone();
            let name = |k: i32| m.data.rigid_bodies.get(k.max(0) as usize).filter(|_| k >= 0).map(|r| r.name.clone()).unwrap_or_else(|| "-".into());
            (j.clone(), m.phys_base.1.get(i).cloned(), name(j.a), name(j.b))
        };
        let mut j = orig.clone();
        let mut changed = false;
        ui.label(RichText::new(&j.name).strong().size(14.0));
        ui.label(RichText::new(format!("joins {a_name} ↔ {b_name}")).size(10.5).color(theme::TEXT_DIM));
        ui.label(RichText::new("Same min and max locks an axis; min bigger than max leaves it free.").size(10.5).color(theme::TEXT_DIM));
        egui::Grid::new(("joint_grid", i)).num_columns(2).spacing(vec2(8.0, 4.0)).show(ui, |ui| {
            ui.label("Move min");
            ui.horizontal(|ui| changed |= vec3_edit(ui, &mut j.lin_min, 0.01, false));
            ui.end_row();
            ui.label("Move max");
            ui.horizontal(|ui| changed |= vec3_edit(ui, &mut j.lin_max, 0.01, false));
            ui.end_row();
            ui.label("Turn min (°)");
            ui.horizontal(|ui| changed |= vec3_edit(ui, &mut j.ang_min, 0.5, true));
            ui.end_row();
            ui.label("Turn max (°)");
            ui.horizontal(|ui| changed |= vec3_edit(ui, &mut j.ang_max, 0.5, true));
            ui.end_row();
            ui.label("Move spring");
            ui.horizontal(|ui| changed |= vec3_edit(ui, &mut j.spring_lin, 0.5, false));
            ui.end_row();
            ui.label("Turn spring");
            ui.horizontal(|ui| changed |= vec3_edit(ui, &mut j.spring_ang, 0.5, false));
            ui.end_row();
        });
        let tweaked = base.as_ref() != Some(&orig);
        if ui.add_enabled(tweaked, egui::Button::new("Revert this joint")).clicked() {
            if let Some(b) = base {
                j = b;
                changed = true;
            }
        }
        if changed {
            self.commit_part(mi, None, Some((i, j)));
        }
    }
}
