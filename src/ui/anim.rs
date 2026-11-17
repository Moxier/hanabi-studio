//! Animation workspace: outliner, bone list, inspector (pose, curves, morphs, tools) and editing actions.

use crate::app::{App, GizmoMode, KeyClip};
use crate::motion::*;
use crate::tools::{self, PoseClip};
use crate::ui::theme;
use egui::{pos2, vec2, Color32, Rect, RichText, Sense, Stroke};
use glam::{Quat, Vec3};

impl App {
    pub fn edit_bone(&mut self, mi: usize, bi: usize, p: Vec3, r: Quat) {
        let frame = self.scene.frame.round() as u32;
        let curves = [self.default_curve; 4];
        let m = &mut self.scene.models[mi];
        m.pose_edit.insert(bi, (p, r));
        m.skel.states[bi].anim_pos = p;
        m.skel.states[bi].anim_rot = r;
        if self.auto_key && self.gizmo_drag.is_none() {
            m.register_bone_key(bi, frame, curves);
        }
    }

    pub fn register_selected(&mut self) {
        let frame = self.scene.frame.round() as u32;
        if self.sel_camera {
            // Enter keys the camera, and the light too only if it was changed.
            let light = self.scene.light_edited;
            self.key_camera();
            if light {
                self.key_light();
                self.set_status(format!("Camera and light keys at frame {frame}"));
            }
            return;
        }
        let selected = self.selected_bones();
        let Some(mi) = self.scene.selected else { return };
        let curves = [self.default_curve; 4];
        self.checkpoint();
        let m = &mut self.scene.models[mi];
        let mut n = 0;
        // Key every edited bone and morph (and the selected bone even if untouched).
        let mut bones: Vec<usize> = m.pose_edit.keys().copied().collect();
        for b in selected {
            if !bones.contains(&b) {
                bones.push(b);
            }
        }
        for b in bones {
            if b < m.data.bones.len() {
                m.register_bone_key(b, frame, curves);
                n += 1;
            }
        }
        let mut morphs: Vec<usize> = m.morph_edit.keys().copied().collect();
        if let Some(s) = self.sel_morph {
            if !morphs.contains(&s) && s < m.data.morphs.len() && self.sel_bone.is_none() {
                morphs.push(s);
            }
        }
        for mo in morphs {
            m.register_morph_key(mo, frame);
            n += 1;
        }
        self.set_status(format!("Registered {n} key(s) at frame {frame}"));
    }

    pub fn key_all_bones(&mut self) {
        let Some(mi) = self.scene.selected else { return };
        self.checkpoint();
        let frame = self.scene.frame.round() as u32;
        let curves = [self.default_curve; 4];
        let m = &mut self.scene.models[mi];
        let ops: Vec<usize> = (0..m.data.bones.len()).filter(|&i| m.data.bones[i].operable && (m.data.bones[i].rotatable || m.data.bones[i].movable)).collect();
        for &i in &ops {
            m.register_bone_key(i, frame, curves);
        }
        self.set_status(format!("Keyed {} bones at frame {frame}", ops.len()));
    }

    pub fn jump_key(&mut self, next: bool) {
        let track = if self.sel_camera {
            Some(TrackId::Camera)
        } else {
            let mi = self.scene.selected;
            match (mi, self.sel_bone) {
                (Some(mi), Some(b)) => self.scene.models.get(mi).and_then(|m| m.data.bones.get(b)).map(|b| TrackId::Bone(b.name.clone())),
                _ => None,
            }
        };
        let frames: Vec<u32> = match &track {
            Some(TrackId::Camera) => self.scene.camera_motion.track_frames(&TrackId::Camera),
            Some(t) => self.scene.selected.map(|mi| self.scene.models[mi].motion.track_frames(t)).unwrap_or_default(),
            None => {
                let mut all: Vec<u32> = self.scene.models.iter().flat_map(|m| m.motion.bones.values().flatten().map(|k| k.frame)).collect();
                all.sort();
                all.dedup();
                all
            }
        };
        let cur = self.scene.frame.round() as u32;
        let f = if next { frames.iter().find(|&&f| f > cur).copied() } else { frames.iter().rev().find(|&&f| f < cur).copied() };
        if let Some(f) = f {
            self.scene.set_frame(f as f32);
        }
    }

    pub fn delete_selected_keys(&mut self) {
        if self.sel_keys.is_empty() {
            return;
        }
        self.checkpoint();
        let mut n = 0;
        let keys = std::mem::take(&mut self.sel_keys);
        for (t, f) in &keys {
            let removed = match t {
                TrackId::Camera | TrackId::Light => self.scene.camera_motion.delete_key(t, *f),
                _ => self.scene.selected.map(|mi| self.scene.models[mi].motion.delete_key(t, *f)).unwrap_or(false),
            };
            n += removed as usize;
        }
        self.set_status(format!("Deleted {n} key(s)"));
    }

    pub fn copy_keys_or_pose(&mut self) {
        let Some(mi) = self.scene.selected else { return };
        if !self.sel_keys.is_empty() {
            let base = self.sel_keys.iter().map(|k| k.1).min().unwrap_or(0) as i64;
            let m = &self.scene.models[mi];
            let mut clip = Vec::new();
            for (t, f) in &self.sel_keys {
                let rel = *f as i64 - base;
                let item = match t {
                    TrackId::Bone(n) => m.motion.bones.get(n).and_then(|v| v.iter().find(|k| k.frame == *f)).cloned().map(KeyClip::Bone),
                    TrackId::Morph(n) => m.motion.morphs.get(n).and_then(|v| v.iter().find(|k| k.frame == *f)).cloned().map(KeyClip::Morph),
                    TrackId::Camera => self.scene.camera_motion.camera.iter().find(|k| k.frame == *f).cloned().map(KeyClip::Camera),
                    TrackId::Light => self.scene.camera_motion.light.iter().find(|k| k.frame == *f).cloned().map(KeyClip::Light),
                };
                if let Some(it) = item {
                    clip.push((t.clone(), rel, it));
                }
            }
            self.set_status(format!("Copied {} key(s)", clip.len()));
            self.key_clip = clip;
            self.pose_clip = None;
        } else {
            let m = &self.scene.models[mi];
            let bones = m
                .data
                .bones
                .iter()
                .enumerate()
                .filter(|(_, b)| b.operable)
                .map(|(i, b)| {
                    let (p, r) = m.bone_pose(i);
                    (b.name.clone(), p, r)
                })
                .collect();
            self.pose_clip = Some(PoseClip { bones });
            self.key_clip.clear();
            self.set_status("Copied the whole pose");
        }
    }

    pub fn paste_keys_or_pose(&mut self, mirrored: bool) {
        let Some(mi) = self.scene.selected else { return };
        let frame = self.scene.frame.round() as i64;
        if !self.key_clip.is_empty() {
            self.checkpoint();
            let clip: Vec<(TrackId, i64, KeyClip)> = self.key_clip.iter().map(|(t, r, k)| (t.clone(), *r, clone_clip(k))).collect();
            let mut n = 0;
            for (t, rel, k) in clip {
                let f = (frame + rel).max(0) as u32;
                match (t, k) {
                    (TrackId::Bone(name), KeyClip::Bone(mut k)) => {
                        k.frame = f;
                        let mut name = name;
                        if mirrored {
                            let (p, r) = tools::mirror_rt(k.pos, k.rot);
                            k.pos = p;
                            k.rot = r;
                            name = crate::model::mirror_name(&name).unwrap_or(name);
                        }
                        self.scene.models[mi].motion.set_bone_key(&name, k);
                    }
                    (TrackId::Morph(name), KeyClip::Morph(mut k)) => {
                        k.frame = f;
                        self.scene.models[mi].motion.set_morph_key(&name, k);
                    }
                    (_, KeyClip::Camera(mut k)) => {
                        k.frame = f;
                        self.scene.camera_motion.set_camera_key(k);
                    }
                    (_, KeyClip::Light(mut k)) => {
                        k.frame = f;
                        self.scene.camera_motion.set_light_key(k);
                    }
                    _ => continue,
                }
                n += 1;
            }
            self.set_status(format!("Pasted {n} key(s) at frame {frame}"));
        } else if let Some(clip) = self.pose_clip.clone() {
            let clip = if mirrored { tools::mirror_pose(&clip) } else { clip };
            let m = &mut self.scene.models[mi];
            for (n, p, r) in &clip.bones {
                if let Some(i) = m.data.bone_index(n) {
                    m.pose_edit.insert(i, (*p, *r));
                }
            }
            self.set_status(if mirrored { "Pasted mirrored pose — press Enter to key it" } else { "Pasted pose — press Enter to key it" });
        }
    }

    pub fn mirror_current_pose(&mut self) {
        let Some(mi) = self.scene.selected else { return };
        let m = &mut self.scene.models[mi];
        let poses: Vec<(String, Vec3, Quat)> = m.data.bones.iter().enumerate().map(|(i, b)| {
            let (p, r) = m.bone_pose(i);
            (b.name.clone(), p, r)
        }).collect();
        let mirrored = tools::mirror_pose(&PoseClip { bones: poses });
        for (n, p, r) in mirrored.bones {
            if let Some(i) = m.data.bone_index(&n) {
                m.pose_edit.insert(i, (p, r));
            }
        }
        self.set_status("Mirrored the pose (left ↔ right). Press Enter to key.");
    }

    pub fn reset_pose(&mut self, only_selected: bool) {
        let Some(mi) = self.scene.selected else { return };
        if only_selected {
            self.reset_selected(true, true);
            return;
        }
        let m = &mut self.scene.models[mi];
        {
            for i in 0..m.data.bones.len() {
                m.pose_edit.insert(i, (Vec3::ZERO, Quat::IDENTITY));
            }
        }
    }

    pub fn animation_ws(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let target = (ui.available_height() * 0.27).clamp(170.0, 330.0);
        let tl_panel = egui::Panel::bottom("timeline").resizable(true).default_size(target).size_range(120.0..=700.0);
        tl_panel.show(ui, |ui| {
            self.timeline(ui);
        });
        egui::Panel::left("outliner").resizable(true).default_size(230.0).size_range(180.0..=420.0).show(ui, |ui| {
            self.outliner(ui);
        });
        egui::Panel::right("inspector").resizable(true).default_size(310.0).size_range(260.0..=480.0).show(ui, |ui| {
            self.inspector(ui);
        });
        egui::CentralPanel::default().frame(egui::Frame::new().fill(theme::BG0)).show(ui, |ui| {
            let rect = ui.max_rect();
            self.viewport_toolbar(ui, rect);
            let vr = Rect::from_min_max(rect.min + vec2(0.0, 30.0), rect.max);
            self.viewport(ui, frame, vr, true);
        });
    }

    fn viewport_toolbar(&mut self, ui: &mut egui::Ui, rect: Rect) {
        let bar = Rect::from_min_size(rect.min, vec2(rect.width(), 30.0));
        ui.scope_builder(egui::UiBuilder::new().max_rect(bar.shrink2(vec2(8.0, 3.0))), |ui| {
            ui.horizontal_centered(|ui| {
                ui.selectable_value(&mut self.gizmo, GizmoMode::Rotate, "Rotate (R)");
                ui.selectable_value(&mut self.gizmo, GizmoMode::Move, "Move (T)");
                ui.toggle_value(&mut self.gizmo_local, "Local axes").on_hover_text("Gizmo follows the bone's own axes");
                ui.separator();
                let ak = egui::Button::new(RichText::new("● Auto key").color(if self.auto_key { Color32::WHITE } else { theme::TEXT_DIM }))
                    .fill(if self.auto_key { Color32::from_rgb(190, 40, 60) } else { theme::BG3 });
                if ui.add(ak).on_hover_text("Every pose edit becomes a keyframe (K)").clicked() {
                    self.auto_key = !self.auto_key;
                }
                if ui.button("Physics…").on_hover_text("Gravity, wind, sudden-move protection").clicked() {
                    self.show_physics = !self.show_physics;
                }
                ui.menu_button("Display…", |ui| {
                    ui.checkbox(&mut self.scene.stage.show_physics_bones, "Physics bones (hair, skirt…)");
                    ui.checkbox(&mut self.scene.stage.show_bones, "Bones");
                    ui.checkbox(&mut self.scene.stage.show_rigid, "Physics shapes");
                    ui.checkbox(&mut self.scene.stage.physics, "Physics simulation");
                    ui.checkbox(&mut self.scene.stage.self_shadow, "Self shadow");
                    ui.checkbox(&mut self.scene.stage.ground_shadow, "Ground shadow");
                    ui.checkbox(&mut self.scene.stage.edges, "Outlines");
                    ui.checkbox(&mut self.scene.stage.show_floor, "Floor");
                    ui.checkbox(&mut self.scene.stage.show_grid, "Grid");
                    ui.checkbox(&mut self.scene.stage.msaa, "Anti-aliasing");
                    let mut follow = self.scene.camera_follows_motion;
                    if ui.checkbox(&mut follow, "Camera follows camera motion").changed() {
                        self.scene.camera_follows_motion = follow;
                    }
                });
                if ui.button("Reset view").on_hover_text("Target 0, 10, 0 · angle 0, 0, 0").clicked() {
                    self.scene.camera = crate::scene::CameraState::default();
                    self.scene.camera_edited = !self.scene.camera_motion.camera.is_empty();
                }
                let n = self.selected_bones().len();
                if !self.sel_camera && n > 1 {
                    ui.label(RichText::new(format!("{n} bones selected")).size(11.0).color(theme::KEY));
                }
            });
        });
    }

    fn outliner(&mut self, ui: &mut egui::Ui) {
        self.scene_panel(ui);
        ui.separator();
        // Bones by display frame
        let Some(mi) = self.scene.selected.filter(|_| !self.sel_camera) else {
            ui.label(RichText::new(if self.sel_camera { "Camera selected: bones are hidden. Pick a model to pose it." } else { "Select a model to see its bones." }).color(theme::TEXT_DIM));
            return;
        };
        if mi >= self.scene.models.len() {
            return;
        }
        ui.horizontal(|ui| {
            theme::section(ui, "Bones & expressions");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.menu_button("Select ⏷", |ui| {
                    for (op, label) in [
                        (crate::ui::animtools::SelOp::All, "All  (A)"),
                        (crate::ui::animtools::SelOp::None, "None  (Alt+A)"),
                        (crate::ui::animtools::SelOp::Invert, "Invert  (Ctrl+I)"),
                        (crate::ui::animtools::SelOp::Mirror, "Add mirror side"),
                        (crate::ui::animtools::SelOp::Children, "Add children"),
                        (crate::ui::animtools::SelOp::Parent, "Add parents"),
                        (crate::ui::animtools::SelOp::Group, "Add same group"),
                        (crate::ui::animtools::SelOp::Keyed, "Keyed at playhead"),
                    ] {
                        if ui.button(label).clicked() {
                            self.select_op(op);
                            ui.close();
                        }
                    }
                });
            });
        });
        ui.add(egui::TextEdit::singleline(&mut self.bone_search).hint_text("🔍 Find bone or expression").desired_width(f32::INFINITY));
        let q = self.bone_search.trim().to_lowercase();
        let frame = self.scene.frame.round() as u32;
        let mods = ui.input(|i| i.modifiers);
        let mut clicked_bone = None;
        egui::ScrollArea::vertical().id_salt("bones").show(ui, |ui| {
            let m = &self.scene.models[mi];
            let data = m.data.clone();
            for (fi, fr) in data.frames.iter().enumerate() {
                if fr.items.is_empty() {
                    continue;
                }
                let matches = |name: &str| q.is_empty() || name.to_lowercase().contains(&q);
                let any = fr.items.iter().any(|&(is_m, i)| {
                    i >= 0 && if is_m { data.morphs.get(i as usize).map(|x| matches(&x.name)).unwrap_or(false) } else { data.bones.get(i as usize).map(|x| matches(&x.name) || matches(&x.name_en)).unwrap_or(false) }
                });
                if !any {
                    continue;
                }
                let id = ui.make_persistent_id(("frame", mi, fi));
                let mut state = egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, fi < 4);
                if !q.is_empty() {
                    state.set_open(true);
                }
                state
                    .show_header(ui, |ui| {
                        ui.label(RichText::new(&fr.name).strong());
                    })
                    .body(|ui| {
                        for &(is_morph, idx) in &fr.items {
                            if idx < 0 {
                                continue;
                            }
                            let idx = idx as usize;
                            if is_morph {
                                let Some(mo) = data.morphs.get(idx) else { continue };
                                if !matches(&mo.name) {
                                    continue;
                                }
                                let keyed = self.scene.models[mi].motion.morph_key_at(&mo.name, frame);
                                let label = format!("{} {}", if keyed { "◆" } else { "◇" }, mo.name);
                                if ui.selectable_label(self.sel_morph == Some(idx) && self.sel_bone.is_none(), label).clicked() {
                                    self.sel_morph = Some(idx);
                                    self.sel_bone = None;
                                    self.insp_tab = 1;
                                }
                            } else {
                                let Some(b) = data.bones.get(idx) else { continue };
                                if !matches(&b.name) && !matches(&b.name_en) {
                                    continue;
                                }
                                if !self.scene.stage.show_physics_bones && self.scene.models[mi].physics_bones.get(idx).copied().unwrap_or(false) {
                                    continue;
                                }
                                let has = self.scene.models[mi].motion.bones.get(&b.name);
                                let keyed = has.map(|v| v.binary_search_by_key(&frame, |k| k.frame).is_ok()).unwrap_or(false);
                                let mark = if keyed { "◆" } else if has.is_some() { "◇" } else { "  " };
                                let tag = if b.ik.is_some() { " (IK)" } else { "" };
                                let col = if keyed {
                                    theme::KEY
                                } else if b.ik.is_some() {
                                    Color32::from_rgb(255, 170, 90)
                                } else if b.movable {
                                    Color32::from_rgb(140, 225, 160)
                                } else {
                                    Color32::from_gray(215)
                                };
                                let active = self.sel_bone == Some(idx);
                                let label = RichText::new(format!("{mark}  {}{tag}", b.name)).color(col);
                                let label = if active { label.strong() } else { label };
                                if ui.selectable_label(self.is_bone_selected(mi, idx), label).on_hover_text("Ctrl+click: add/remove · Shift+click: add").clicked() {
                                    clicked_bone = Some(idx);
                                }
                            }
                        }
                    });
            }
        });
        if let Some(b) = clicked_bone {
            self.click_bone(b, mods);
            self.insp_tab = 0;
        }
    }

    fn inspector(&mut self, ui: &mut egui::Ui) {
        if self.sel_camera {
            egui::ScrollArea::vertical().show(ui, |ui| self.camera_inspector(ui));
            return;
        }
        let Some(mi) = self.scene.selected.filter(|&i| i < self.scene.models.len()) else {
            ui.add_space(12.0);
            ui.label(RichText::new("Nothing selected. Pick a model or Camera & Light in the Scene list.").color(theme::TEXT_DIM));
            return;
        };
        self.model_header(ui, mi);
        if self.scene.models[mi].is_accessory() {
            egui::ScrollArea::vertical().show(ui, |ui| self.accessory_inspector(ui, mi));
            return;
        }
        // Tabs
        ui.horizontal(|ui| {
            for (t, label, tip) in [(0u8, "Pose", "Bone values, keys and curves"), (1u8, "Expressions", "Morph sliders"), (2u8, "Tools", "Selection, multiply/offset, timing")] {
                let sel = self.insp_tab == t;
                let b = egui::Button::new(RichText::new(label).size(13.0).color(if sel { Color32::WHITE } else { theme::TEXT_DIM }))
                    .fill(if sel { theme::ACCENT.linear_multiply(0.35) } else { theme::BG2 })
                    .min_size(vec2((ui.available_width() - 8.0) / 3.0, 26.0).max(vec2(60.0, 26.0)));
                if ui.add(b).on_hover_text(tip).clicked() {
                    self.insp_tab = t;
                }
            }
        });
        ui.add_space(2.0);
        egui::ScrollArea::vertical().id_salt(("insp", self.insp_tab)).show(ui, |ui| match self.insp_tab {
            1 => self.morph_tab(ui, mi),
            2 => self.tools_panel(ui),
            _ => self.pose_tab(ui, mi),
        });
    }

    fn model_header(&mut self, ui: &mut egui::Ui, mi: usize) {
        theme::card(ui, |ui| {
            ui.set_width(ui.available_width());
            let m = &mut self.scene.models[mi];
            ui.horizontal(|ui| {
                ui.add(egui::Label::new(RichText::new(m.name()).strong().size(14.0)).truncate());
            });
            egui::CollapsingHeader::new(RichText::new("Model settings").size(11.5)).id_salt(("model_settings", m.id)).show(ui, |ui| {
                if !m.data.comment.is_empty() {
                    ui.label(RichText::new(m.data.comment.lines().next().unwrap_or("")).size(11.0).color(theme::TEXT_DIM));
                }
                ui.label(RichText::new(format!(
                    "{} vertices · {} bones · {} morphs · {} bodies",
                    m.data.vertices.len(),
                    m.data.bones.len(),
                    m.data.morphs.len(),
                    m.data.rigid_bodies.len()
                )).size(11.0).color(theme::TEXT_DIM));
                ui.horizontal(|ui| {
                    ui.checkbox(&mut m.visible, "Visible");
                    ui.checkbox(&mut m.physics_enabled, "Physics");
                });
                if !m.is_accessory() {
                    let r = ui.add(egui::Slider::new(&mut m.edge_width, 0.0..=3.0).text("Outline width"));
                    theme::dbl(r, &mut m.edge_width, 1.0);
                }
            });
        });
    }

    fn pose_tab(&mut self, ui: &mut egui::Ui, mi: usize) {
        let frame = self.scene.frame.round() as u32;
        let selected = self.selected_bones();
        let Some(bi) = self.sel_bone.filter(|&b| b < self.scene.models[mi].data.bones.len()) else {
            theme::card(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new("No bone selected").strong());
                ui.label(RichText::new("Click a bone in the view or in the list. Ctrl+click or Shift+click to pick several; A selects all.").size(11.0).color(theme::TEXT_DIM));
            });
            return;
        };
        let mut changed = None;
        theme::card(ui, |ui| {
            ui.set_width(ui.available_width());
            let m = &self.scene.models[mi];
            let b = &m.data.bones[bi];
            ui.horizontal(|ui| {
                ui.label(RichText::new(&b.name).strong().size(15.0).color(theme::ACCENT));
                if !b.name_en.is_empty() && b.name_en != b.name {
                    ui.label(RichText::new(&b.name_en).size(11.0).color(theme::TEXT_DIM));
                }
            });
            if selected.len() > 1 {
                ui.label(RichText::new(format!("+ {} more selected (keys and resets apply to all)", selected.len() - 1)).size(10.5).color(theme::KEY));
            }
            let (p, r) = m.bone_pose(bi);
            let mut e = euler_deg(r);
            let mut pp = p;
            egui::Grid::new(("bone_vals", bi)).num_columns(4).spacing(vec2(6.0, 4.0)).show(ui, |ui| {
                if b.rotatable {
                    if theme::reset_label(ui, RichText::new("Rotation °").size(11.5).color(theme::TEXT_DIM)) {
                        changed = Some((pp, Quat::IDENTITY));
                    }
                    let mut ch = false;
                    for (k, c) in [theme::AXIS_X, theme::AXIS_Y, theme::AXIS_Z].into_iter().enumerate() {
                        ui.horizontal(|ui| {
                            if theme::reset_label(ui, RichText::new(["X", "Y", "Z"][k]).color(c).strong()) {
                                e[k] = 0.0;
                                ch = true;
                            }
                            ch |= ui.add(egui::DragValue::new(&mut e[k]).speed(0.5).fixed_decimals(1)).changed();
                        });
                    }
                    if ch {
                        changed = Some((pp, quat_from_euler_deg(e)));
                    }
                    ui.end_row();
                }
                if b.movable {
                    if theme::reset_label(ui, RichText::new("Position").size(11.5).color(theme::TEXT_DIM)) {
                        changed = Some((Vec3::ZERO, r));
                    }
                    let mut ch = false;
                    for (k, c) in [theme::AXIS_X, theme::AXIS_Y, theme::AXIS_Z].into_iter().enumerate() {
                        ui.horizontal(|ui| {
                            if theme::reset_label(ui, RichText::new(["X", "Y", "Z"][k]).color(c).strong()) {
                                pp[k] = 0.0;
                                ch = true;
                            }
                            ch |= ui.add(egui::DragValue::new(&mut pp[k]).speed(0.02).fixed_decimals(2)).changed();
                        });
                    }
                    if ch {
                        changed = Some((pp, r));
                    }
                    ui.end_row();
                }
            });
            ui.label(RichText::new("Double-click a label to reset it to 0.").size(10.0).color(theme::TEXT_DIM));
            if b.ik.is_some() {
                let mut on = m.skel.ik_enabled[bi];
                if ui.checkbox(&mut on, "IK enabled").changed() {
                    changed = None;
                    self.scene.models[mi].skel.ik_enabled[bi] = on;
                }
            }
        });
        if let Some((p, r)) = changed {
            self.edit_bone(mi, bi, p, r);
        }
        ui.horizontal_wrapped(|ui| {
            let label = if selected.len() > 1 { format!("◆ Key {} bones  ⏎", selected.len()) } else { "◆ Register key  ⏎".to_string() };
            if ui.add(egui::Button::new(RichText::new(label).color(Color32::BLACK)).fill(theme::KEY)).clicked() {
                self.register_selected();
            }
            if ui.button("Reset rot").on_hover_text("Reset rotation of the selected bones (Alt+R)").clicked() {
                self.reset_selected(false, true);
            }
            if ui.button("Reset pos").on_hover_text("Reset position of the selected bones (Alt+G)").clicked() {
                self.reset_selected(true, false);
            }
            let name = self.scene.models[mi].data.bones[bi].name.clone();
            let has_key = self.scene.models[mi].motion.bone_key_at(&name, frame);
            if ui.add_enabled(has_key || selected.len() > 1, egui::Button::new("Delete key")).on_hover_text("Delete the selected bones' keys at this frame").clicked() {
                self.delete_selected_bone_keys();
            }
        });
        self.curve_editor(ui, mi, bi);
    }

    fn morph_tab(&mut self, ui: &mut egui::Ui, mi: usize) {
        let frame = self.scene.frame.round() as u32;
        ui.add(egui::TextEdit::singleline(&mut self.morph_search).hint_text("🔍 Find expression").desired_width(f32::INFINITY));
        let q = self.morph_search.trim().to_lowercase();
        let data = self.scene.models[mi].data.clone();
        if data.morphs.is_empty() {
            ui.label(RichText::new("This model has no expressions (morphs)").color(theme::TEXT_DIM));
        }
        ui.label(RichText::new("◇ = click to key · double-click a slider to reset it to 0").size(10.0).color(theme::TEXT_DIM));
        for (panel, title) in [(1u8, "Eyebrows"), (2, "Eyes"), (3, "Mouth"), (4, "Other"), (0, "System")] {
            let list: Vec<usize> = (0..data.morphs.len()).filter(|&i| data.morphs[i].panel == panel && (q.is_empty() || data.morphs[i].name.to_lowercase().contains(&q) || data.morphs[i].name_en.to_lowercase().contains(&q))).collect();
            if list.is_empty() {
                continue;
            }
            egui::CollapsingHeader::new(RichText::new(format!("{title} ({})", list.len())).size(12.0)).id_salt(("morphs", panel)).default_open(true).show(ui, |ui| {
                for i in list {
                    let name = data.morphs[i].name.clone();
                    ui.horizontal(|ui| {
                        let keyed = self.scene.models[mi].motion.morph_key_at(&name, frame);
                        let diamond = egui::Button::new(RichText::new(if keyed { "◆" } else { "◇" }).color(if keyed { theme::KEY } else { theme::TEXT_DIM })).frame(false);
                        if ui.add(diamond).on_hover_text("Register key").clicked() {
                            self.checkpoint();
                            self.scene.models[mi].register_morph_key(i, frame);
                        }
                        let mut w = self.scene.models[mi].morph_weights[i];
                        let resp = ui.add(egui::Slider::new(&mut w, 0.0..=1.0).text(&name).fixed_decimals(2));
                        let resp = theme::dbl(resp, &mut w, 0.0);
                        if resp.changed() {
                            self.sel_morph = Some(i);
                            self.scene.models[mi].morph_edit.insert(i, w);
                            if self.auto_key {
                                self.scene.models[mi].morph_weights[i] = w;
                                self.scene.models[mi].register_morph_key(i, frame);
                            }
                        }
                    });
                }
            });
        }
    }

    pub fn generate_motion(&mut self, g: tools::Generator, start: u32, len: u32, bpm: f32) -> usize {
        let Some(mi) = self.scene.selected else { return 0 };
        self.checkpoint();
        let data = self.scene.models[mi].data.clone();
        let fps = self.scene.fps;
        let n = tools::generate(g, &data, &mut self.scene.models[mi].motion, start, len, bpm, fps);
        self.scene.play_end = self.scene.play_end.max(start + len);
        if n == 0 {
            self.set_error("Could not find matching bones on this model for that motion");
        } else {
            self.set_status(format!("{}: created {n} keys", g.label()));
        }
        n
    }

    pub fn accessory_inspector(&mut self, ui: &mut egui::Ui, mi: usize) {
        theme::section(ui, "Placement");
        let parents: Vec<(u64, String, Vec<String>)> = self
            .scene
            .models
            .iter()
            .filter(|m| !m.is_accessory())
            .map(|m| (m.id, m.name().to_string(), m.data.bones.iter().map(|b| b.name.clone()).collect()))
            .collect();
        theme::card(ui, |ui| {
            let m = &mut self.scene.models[mi];
            let cur = m.attach;
            let label = match cur {
                Some((pid, bi)) => parents.iter().find(|p| p.0 == pid).map(|p| format!("{} → {}", p.1, p.2.get(bi).cloned().unwrap_or_default())).unwrap_or("Ground".into()),
                None => "Ground".into(),
            };
            egui::ComboBox::from_label("Attach to").selected_text(label).width(200.0).show_ui(ui, |ui| {
                if ui.selectable_label(cur.is_none(), "Ground").clicked() {
                    m.attach = None;
                }
                for (pid, pname, bones) in &parents {
                    ui.menu_button(pname, |ui| {
                        egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
                            for (bi, bn) in bones.iter().enumerate() {
                                if ui.button(bn).clicked() {
                                    m.attach = Some((*pid, bi));
                                    ui.close();
                                }
                            }
                        });
                    });
                }
            });
            ui.horizontal(|ui| {
                if theme::reset_label(ui, RichText::new("Position")) {
                    m.acc_pos = Vec3::ZERO;
                }
                ui.add(egui::DragValue::new(&mut m.acc_pos.x).speed(0.05));
                ui.add(egui::DragValue::new(&mut m.acc_pos.y).speed(0.05));
                ui.add(egui::DragValue::new(&mut m.acc_pos.z).speed(0.05));
            });
            ui.horizontal(|ui| {
                if theme::reset_label(ui, RichText::new("Rotation")) {
                    m.acc_rot = Vec3::ZERO;
                }
                ui.add(egui::DragValue::new(&mut m.acc_rot.x).speed(0.5).suffix("°"));
                ui.add(egui::DragValue::new(&mut m.acc_rot.y).speed(0.5).suffix("°"));
                ui.add(egui::DragValue::new(&mut m.acc_rot.z).speed(0.5).suffix("°"));
            });
            theme::dbl(ui.add(egui::Slider::new(&mut m.acc_scale, 0.01..=50.0).logarithmic(true).text("Scale")), &mut m.acc_scale, 10.0);
            theme::dbl(ui.add(egui::Slider::new(&mut m.acc_opacity, 0.0..=1.0).text("Opacity")), &mut m.acc_opacity, 1.0);
        });
    }

    pub fn key_camera(&mut self) {
        let frame = self.scene.frame.round() as u32;
        self.checkpoint();
        let k = self.scene.camera.to_key(frame, [Curve::EASE_IN_OUT; 6]);
        self.scene.camera_motion.set_camera_key(k);
        self.scene.camera_edited = false;
        self.set_status(format!("Camera key at frame {frame}"));
    }

    pub fn key_light(&mut self) {
        let frame = self.scene.frame.round() as u32;
        self.checkpoint();
        let lk = LightKey { frame, color: self.scene.light.color, direction: self.scene.light.direction };
        self.scene.camera_motion.set_light_key(lk);
        self.scene.light_edited = false;
        self.set_status(format!("Light key at frame {frame}"));
    }

    pub fn camera_inspector(&mut self, ui: &mut egui::Ui) {
        let frame = self.scene.frame.round() as u32;
        let cam_keyed = self.scene.camera_motion.camera.iter().any(|k| k.frame == frame);
        let light_keyed = self.scene.camera_motion.light.iter().any(|k| k.frame == frame);
        let d = crate::scene::CameraState::default();
        // ── Camera
        ui.horizontal(|ui| {
            theme::section(ui, "Camera");
            if self.scene.camera_edited {
                ui.label(RichText::new("● moved, not keyed").size(10.5).color(theme::KEY));
            }
        });
        let mut edited = false;
        theme::card(ui, |ui| {
            ui.set_width(ui.available_width());
            let c = &mut self.scene.camera;
            let mut rot = c.rot * (180.0 / std::f32::consts::PI);
            egui::Grid::new("cam").num_columns(4).spacing(vec2(6.0, 4.0)).show(ui, |ui| {
                if theme::reset_label(ui, RichText::new("Target").color(theme::TEXT_DIM)) {
                    c.target = d.target;
                    edited = true;
                }
                for k in 0..3 {
                    edited |= ui.add(egui::DragValue::new(&mut c.target[k]).speed(0.1).max_decimals(2)).changed();
                }
                ui.end_row();
                if theme::reset_label(ui, RichText::new("Angle °").color(theme::TEXT_DIM)) {
                    rot = d.rot * (180.0 / std::f32::consts::PI);
                    edited = true;
                }
                for k in 0..3 {
                    edited |= ui.add(egui::DragValue::new(&mut rot[k]).speed(0.5).max_decimals(1)).changed();
                }
                ui.end_row();
            });
            c.rot = rot * (std::f32::consts::PI / 180.0);
            edited |= theme::dbl(ui.add(egui::Slider::new(&mut c.distance, -400.0..=-1.0).text("Distance")), &mut c.distance, d.distance).changed();
            edited |= theme::dbl(ui.add(egui::Slider::new(&mut c.fov, 5.0..=120.0).text("Field of view")), &mut c.fov, d.fov).changed();
            edited |= ui.checkbox(&mut c.ortho, "Orthographic").changed();
            ui.label(RichText::new("Double-click a label or slider to reset it.").size(10.0).color(theme::TEXT_DIM));
        });
        if edited {
            self.scene.camera_edited = true;
        }
        ui.horizontal(|ui| {
            if ui.add(egui::Button::new(RichText::new("◆ Key camera  ⏎").color(Color32::BLACK)).fill(theme::KEY)).on_hover_text("Register a camera key at this frame").clicked() {
                self.key_camera();
            }
            if ui.add_enabled(cam_keyed, egui::Button::new("Delete camera key")).clicked() {
                self.checkpoint();
                self.scene.camera_motion.delete_key(&TrackId::Camera, frame);
            }
        });
        // ── Light
        let ld = crate::scene::LightState::default();
        ui.horizontal(|ui| {
            theme::section(ui, "Light");
            if self.scene.light_edited {
                ui.label(RichText::new("● changed, not keyed").size(10.5).color(theme::KEY));
            }
        });
        theme::card(ui, |ui| {
            ui.set_width(ui.available_width());
            let l = &mut self.scene.light;
            let mut col = l.color.to_array();
            let mut ch = false;
            ui.horizontal(|ui| {
                if theme::reset_label(ui, RichText::new("Colour").color(theme::TEXT_DIM)) {
                    col = ld.color.to_array();
                    ch = true;
                }
                ch |= ui.color_edit_button_rgb(&mut col).changed();
            });
            l.color = Vec3::from(col);
            ui.horizontal(|ui| {
                if theme::reset_label(ui, RichText::new("Direction").color(theme::TEXT_DIM)) {
                    l.direction = ld.direction;
                    ch = true;
                }
                for k in 0..3 {
                    ch |= ui.add(egui::DragValue::new(&mut l.direction[k]).speed(0.01).range(-1.0..=1.0)).changed();
                }
            });
            if ch {
                self.scene.light_edited = true;
            }
        });
        ui.horizontal(|ui| {
            if ui.add(egui::Button::new(RichText::new("◆ Key light").color(Color32::BLACK)).fill(theme::KEY)).on_hover_text("Register a light key at this frame").clicked() {
                self.key_light();
            }
            if ui.add_enabled(light_keyed, egui::Button::new("Delete light key")).clicked() {
                self.checkpoint();
                self.scene.camera_motion.delete_key(&TrackId::Light, frame);
            }
        });
        // ── Stage
        theme::section(ui, "Stage");
        theme::card(ui, |ui| {
            ui.set_width(ui.available_width());
            let st = &mut self.scene.stage;
            ui.horizontal(|ui| {
                ui.label("Background");
                ui.color_edit_button_rgb(&mut st.background);
            });
            let sd = crate::scene::StageSettings::default().shadow_distance;
            theme::dbl(ui.add(egui::Slider::new(&mut st.shadow_distance, 10.0..=300.0).text("Shadow range")), &mut st.shadow_distance, sd);
        });
        ui.label(RichText::new("Enter keys the camera (and the light too if you changed it). Keys are saved with the camera motion (File → Export camera & light motion).").size(10.5).color(theme::TEXT_DIM));
    }

    /// Bezier interpolation editor for the key at the playhead (or the curve for new keys).
    fn curve_editor(&mut self, ui: &mut egui::Ui, mi: usize, bi: usize) {
        theme::section(ui, "Interpolation");
        let frame = self.scene.frame.round() as u32;
        let name = self.scene.models[mi].data.bones[bi].name.clone();
        let key_idx = self.scene.models[mi].motion.bones.get(&name).and_then(|v| v.iter().position(|k| k.frame == frame));
        theme::card(ui, |ui| {
            let editing_key = key_idx.is_some();
            ui.label(
                RichText::new(if editing_key { format!("Curve into the key at frame {frame}") } else { "Curve used for new keys".to_string() })
                    .size(11.5)
                    .color(theme::TEXT_DIM),
            );
            let mut curves = match key_idx {
                Some(k) => self.scene.models[mi].motion.bones[&name][k].curves,
                None => [self.default_curve; 4],
            };
            let before = curves;
            ui.horizontal(|ui| {
                let current = curves[3];
                for (n, c) in Curve::presets() {
                    if curve_icon(ui, c, current == c, n).clicked() {
                        curves = [c; 4];
                    }
                }
            });
            let size = vec2(ui.available_width().min(270.0), 150.0);
            let (rect, resp) = ui.allocate_exact_size(size, Sense::click_and_drag());
            let p = ui.painter_at(rect);
            p.rect_filled(rect, 4.0, theme::BG0);
            for k in 1..4 {
                let x = rect.left() + rect.width() * k as f32 / 4.0;
                let y = rect.top() + rect.height() * k as f32 / 4.0;
                p.line_segment([pos2(x, rect.top()), pos2(x, rect.bottom())], Stroke::new(1.0, theme::BG2));
                p.line_segment([pos2(rect.left(), y), pos2(rect.right(), y)], Stroke::new(1.0, theme::BG2));
            }
            let to_s = |x: f32, y: f32| pos2(rect.left() + x * rect.width(), rect.bottom() - y * rect.height());
            let cols = [theme::AXIS_X, theme::AXIS_Y, theme::AXIS_Z, theme::KEY];
            let shown: Vec<usize> = if self.curve_all_channels { vec![3] } else { vec![0, 1, 2, 3] };
            for &ci in &shown {
                let c = curves[ci];
                let pts: Vec<egui::Pos2> = (0..=40).map(|i| {
                    let x = i as f32 / 40.0;
                    to_s(x, c.eval(x))
                }).collect();
                p.add(egui::Shape::line(pts, Stroke::new(2.0, cols[ci])));
            }
            let c = curves[shown[shown.len() - 1]];
            let h1 = to_s(c.x1 as f32 / 127.0, c.y1 as f32 / 127.0);
            let h2 = to_s(c.x2 as f32 / 127.0, c.y2 as f32 / 127.0);
            p.line_segment([to_s(0.0, 0.0), h1], Stroke::new(1.0, theme::TEXT_DIM));
            p.line_segment([to_s(1.0, 1.0), h2], Stroke::new(1.0, theme::TEXT_DIM));
            p.circle_filled(h1, 5.0, theme::ACCENT);
            p.circle_filled(h2, 5.0, theme::ACCENT2);
            if resp.dragged() {
                if let Some(mp) = resp.interact_pointer_pos() {
                    let x = ((mp.x - rect.left()) / rect.width()).clamp(0.0, 1.0);
                    let y = ((rect.bottom() - mp.y) / rect.height()).clamp(0.0, 1.0);
                    let first = (mp - h1).length() < (mp - h2).length();
                    for &ci in &shown {
                        let cc = &mut curves[ci];
                        let (xv, yv) = ((x * 127.0).round() as u8, (y * 127.0).round() as u8);
                        if first {
                            cc.x1 = xv;
                            cc.y1 = yv;
                        } else {
                            cc.x2 = xv;
                            cc.y2 = yv;
                        }
                        if self.curve_all_channels {
                            curves = [*cc; 4];
                            break;
                        }
                    }
                }
            }
            ui.checkbox(&mut self.curve_all_channels, "Same curve for move & rotate");
            if curves != before {
                match key_idx {
                    Some(k) => {
                        if resp.drag_started() || !resp.dragged() {
                            self.checkpoint();
                        }
                        if let Some(v) = self.scene.models[mi].motion.bones.get_mut(&name) {
                            v[k].curves = curves;
                        }
                    }
                    None => self.default_curve = curves[3],
                }
            }
        });
    }
}

/// A small button showing the shape of an easing curve (name in the tooltip).
pub fn curve_icon(ui: &mut egui::Ui, curve: Curve, selected: bool, name: &str) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(30.0, 22.0), Sense::click());
    let p = ui.painter_at(rect);
    let hot = resp.hovered();
    let fill = if selected { theme::ACCENT.linear_multiply(0.45) } else if hot { theme::BG3 } else { theme::BG2 };
    p.rect_filled(rect, 4.0, fill);
    p.rect_stroke(rect, 4.0, Stroke::new(1.0, if selected { theme::ACCENT } else { theme::BG3 }), egui::StrokeKind::Inside);
    let r = rect.shrink2(vec2(6.0, 4.0));
    let pts: Vec<egui::Pos2> = (0..=16)
        .map(|i| {
            let x = i as f32 / 16.0;
            let y = curve.eval(x).clamp(-0.1, 1.1);
            pos2(r.left() + x * r.width(), r.bottom() - y * r.height())
        })
        .collect();
    p.add(egui::Shape::line(pts, Stroke::new(1.8, if selected { Color32::WHITE } else { theme::KEY })));
    resp.on_hover_text(name)
}

fn clone_clip(k: &KeyClip) -> KeyClip {
    match k {
        KeyClip::Bone(b) => KeyClip::Bone(b.clone()),
        KeyClip::Morph(b) => KeyClip::Morph(b.clone()),
        KeyClip::Camera(b) => KeyClip::Camera(b.clone()),
        KeyClip::Light(b) => KeyClip::Light(b.clone()),
    }
}
