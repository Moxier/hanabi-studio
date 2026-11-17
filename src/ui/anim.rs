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
            self.checkpoint();
            let k = self.scene.camera.to_key(frame, [Curve::EASE_IN_OUT; 6]);
            self.scene.camera_motion.set_camera_key(k);
            let lk = LightKey { frame, color: self.scene.light.color, direction: self.scene.light.direction };
            self.scene.camera_motion.set_light_key(lk);
            self.scene.camera_edited = false;
            self.scene.light_edited = false;
            self.set_status(format!("Camera & light key at frame {frame}"));
            return;
        }
        let Some(mi) = self.scene.selected else { return };
        let curves = [self.default_curve; 4];
        self.checkpoint();
        let m = &mut self.scene.models[mi];
        let mut n = 0;
        // Key every edited bone and morph (and the selected bone even if untouched).
        let mut bones: Vec<usize> = m.pose_edit.keys().copied().collect();
        if let Some(b) = self.sel_bone {
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
        let m = &mut self.scene.models[mi];
        if only_selected {
            if let Some(b) = self.sel_bone {
                m.pose_edit.insert(b, (Vec3::ZERO, Quat::IDENTITY));
            }
        } else {
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
            egui::ScrollArea::vertical().show(ui, |ui| self.inspector(ui));
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
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Reset view").clicked() {
                        self.scene.camera = crate::scene::CameraState::default();
                        self.scene.camera_edited = !self.scene.camera_motion.camera.is_empty();
                    }
                });
            });
        });
    }

    fn outliner(&mut self, ui: &mut egui::Ui) {
        self.scene_panel(ui);
        ui.separator();
        // Bones by display frame
        let Some(mi) = self.scene.selected.filter(|_| !self.sel_camera) else {
            ui.label(RichText::new("Select a model to see its bones.").color(theme::TEXT_DIM));
            return;
        };
        if mi >= self.scene.models.len() {
            return;
        }
        theme::section(ui, "Bones & expressions");
        let frame = self.scene.frame.round() as u32;
        egui::ScrollArea::vertical().id_salt("bones").show(ui, |ui| {
            let m = &self.scene.models[mi];
            let data = m.data.clone();
            for (fi, fr) in data.frames.iter().enumerate() {
                if fr.items.is_empty() {
                    continue;
                }
                let id = ui.make_persistent_id(("frame", mi, fi));
                egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, fi < 4)
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
                                let keyed = self.scene.models[mi].motion.morph_key_at(&mo.name, frame);
                                let label = format!("{} {}", if keyed { "◆" } else { "◇" }, mo.name);
                                if ui.selectable_label(self.sel_morph == Some(idx) && self.sel_bone.is_none(), label).clicked() {
                                    self.sel_morph = Some(idx);
                                    self.sel_bone = None;
                                }
                            } else {
                                let Some(b) = data.bones.get(idx) else { continue };
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
                                let label = RichText::new(format!("{mark}  {}{tag}", b.name)).color(col);
                                if ui.selectable_label(self.sel_bone == Some(idx), label).clicked() {
                                    self.sel_bone = Some(idx);
                                }
                            }
                        }
                    });
            }
        });
    }

    fn inspector(&mut self, ui: &mut egui::Ui) {
        if self.sel_camera {
            self.camera_inspector(ui);
            return;
        }
        let Some(mi) = self.scene.selected.filter(|&i| i < self.scene.models.len()) else {
            ui.label(RichText::new("Nothing selected").color(theme::TEXT_DIM));
            return;
        };
        let frame = self.scene.frame.round() as u32;
        // Model card
        theme::section(ui, "Model");
        theme::card(ui, |ui| {
            let m = &mut self.scene.models[mi];
            ui.label(RichText::new(m.name()).strong().size(14.0));
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
                ui.add(egui::Slider::new(&mut m.edge_width, 0.0..=3.0).text("Outline width"));
            }
        });
        if self.scene.models[mi].is_accessory() {
            self.accessory_inspector(ui, mi);
            return;
        }
        // Bone card
        if let Some(bi) = self.sel_bone.filter(|&b| b < self.scene.models[mi].data.bones.len()) {
            theme::section(ui, "Bone");
            let mut changed = None;
            theme::card(ui, |ui| {
                let m = &self.scene.models[mi];
                let b = &m.data.bones[bi];
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&b.name).strong().size(14.0).color(theme::ACCENT));
                    if !b.name_en.is_empty() && b.name_en != b.name {
                        ui.label(RichText::new(&b.name_en).size(11.0).color(theme::TEXT_DIM));
                    }
                });
                let (p, r) = m.bone_pose(bi);
                let mut e = euler_deg(r);
                let mut pp = p;
                if b.rotatable {
                    ui.label(RichText::new("Rotation (°)").size(11.0).color(theme::TEXT_DIM));
                    ui.horizontal(|ui| {
                        let mut ch = false;
                        for (v, c, n) in [(&mut e.x, theme::AXIS_X, "X"), (&mut e.y, theme::AXIS_Y, "Y"), (&mut e.z, theme::AXIS_Z, "Z")] {
                            ui.label(RichText::new(n).color(c));
                            ch |= ui.add(egui::DragValue::new(v).speed(0.5).fixed_decimals(1)).changed();
                        }
                        if ch {
                            changed = Some((pp, quat_from_euler_deg(e)));
                        }
                    });
                }
                if b.movable {
                    ui.label(RichText::new("Position").size(11.0).color(theme::TEXT_DIM));
                    ui.horizontal(|ui| {
                        let mut ch = false;
                        for (v, c, n) in [(&mut pp.x, theme::AXIS_X, "X"), (&mut pp.y, theme::AXIS_Y, "Y"), (&mut pp.z, theme::AXIS_Z, "Z")] {
                            ui.label(RichText::new(n).color(c));
                            ch |= ui.add(egui::DragValue::new(v).speed(0.02).fixed_decimals(2)).changed();
                        }
                        if ch {
                            changed = Some((pp, r));
                        }
                    });
                }
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
                if ui.add(egui::Button::new(RichText::new("◆ Register key  ⏎").color(Color32::BLACK)).fill(theme::KEY)).clicked() {
                    self.register_selected();
                }
                if ui.button("Reset bone").clicked() {
                    self.reset_pose(true);
                }
                let name = self.scene.models[mi].data.bones[bi].name.clone();
                let has_key = self.scene.models[mi].motion.bone_key_at(&name, frame);
                if ui.add_enabled(has_key, egui::Button::new("Delete key")).clicked() {
                    self.checkpoint();
                    self.scene.models[mi].motion.delete_key(&TrackId::Bone(name), frame);
                }
            });
            self.curve_editor(ui, mi, bi);
        }
        // Morphs
        theme::section(ui, "Expressions (morphs)");
        theme::card(ui, |ui| {
            let data = self.scene.models[mi].data.clone();
            if data.morphs.is_empty() {
                ui.label(RichText::new("This model has no morphs").color(theme::TEXT_DIM));
            }
            for (panel, title) in [(1u8, "Eyebrows"), (2, "Eyes"), (3, "Mouth"), (4, "Other"), (0, "System")] {
                let list: Vec<usize> = (0..data.morphs.len()).filter(|&i| data.morphs[i].panel == panel).collect();
                if list.is_empty() {
                    continue;
                }
                egui::CollapsingHeader::new(RichText::new(format!("{title} ({})", list.len())).size(12.0)).default_open(list.len() <= 12).show(ui, |ui| {
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
        });
        self.tools_card(ui);
    }

    fn tools_card(&mut self, ui: &mut egui::Ui) {
        theme::section(ui, "Quick tools");
        theme::card(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                if ui.button("⇋ Mirror pose").on_hover_text("Swap left and right").clicked() {
                    self.mirror_current_pose();
                }
                if ui.button("Copy pose").clicked() {
                    self.sel_keys.clear();
                    self.copy_keys_or_pose();
                }
                if ui.add_enabled(self.pose_clip.is_some(), egui::Button::new("Paste pose")).clicked() {
                    self.paste_keys_or_pose(false);
                }
                if ui.add_enabled(self.pose_clip.is_some(), egui::Button::new("Paste mirrored")).clicked() {
                    self.paste_keys_or_pose(true);
                }
                if ui.button("Reset pose").clicked() {
                    self.reset_pose(false);
                }
                if ui.button("Key all bones").clicked() {
                    self.key_all_bones();
                }
            });
            ui.separator();
            ui.label(RichText::new("Generate motion at the playhead").size(11.5).color(theme::TEXT_DIM));
            ui.horizontal(|ui| {
                egui::ComboBox::from_id_salt("gen").selected_text(self.gen_kind.label()).show_ui(ui, |ui| {
                    for g in tools::Generator::ALL {
                        ui.selectable_value(&mut self.gen_kind, g, g.label());
                    }
                });
                ui.add(egui::DragValue::new(&mut self.gen_length).range(10..=3000).suffix(" fr"));
            });
            ui.horizontal(|ui| {
                if self.gen_kind == tools::Generator::Bounce {
                    ui.add(egui::DragValue::new(&mut self.gen_bpm).range(40.0..=240.0).suffix(" BPM"));
                }
                if ui.button("✨ Generate").clicked() {
                    self.generate_motion(self.gen_kind, self.scene.frame.round() as u32, self.gen_length, self.gen_bpm);
                }
            });
            ui.separator();
            if ui.button("Smooth selected bone's keys").on_hover_text("Softens jittery captured motion").clicked() {
                if let (Some(mi), Some(bi)) = (self.scene.selected, self.sel_bone) {
                    self.checkpoint();
                    let name = self.scene.models[mi].data.bones[bi].name.clone();
                    if let Some(v) = self.scene.models[mi].motion.bones.get_mut(&name) {
                        tools::smooth_track(v, 0.8);
                    }
                    self.set_status(format!("Smoothed keys on {name}"));
                }
            }
        });
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
                ui.label("Position");
                ui.add(egui::DragValue::new(&mut m.acc_pos.x).speed(0.05));
                ui.add(egui::DragValue::new(&mut m.acc_pos.y).speed(0.05));
                ui.add(egui::DragValue::new(&mut m.acc_pos.z).speed(0.05));
            });
            ui.horizontal(|ui| {
                ui.label("Rotation");
                ui.add(egui::DragValue::new(&mut m.acc_rot.x).speed(0.5).suffix("°"));
                ui.add(egui::DragValue::new(&mut m.acc_rot.y).speed(0.5).suffix("°"));
                ui.add(egui::DragValue::new(&mut m.acc_rot.z).speed(0.5).suffix("°"));
            });
            ui.add(egui::Slider::new(&mut m.acc_scale, 0.01..=50.0).logarithmic(true).text("Scale"));
            ui.add(egui::Slider::new(&mut m.acc_opacity, 0.0..=1.0).text("Opacity"));
        });
    }

    pub fn camera_inspector(&mut self, ui: &mut egui::Ui) {
        theme::section(ui, "Camera");
        let mut edited = false;
        theme::card(ui, |ui| {
            let c = &mut self.scene.camera;
            let mut rot = c.rot * (180.0 / std::f32::consts::PI);
            egui::Grid::new("cam").num_columns(4).show(ui, |ui| {
                ui.label("Target");
                edited |= ui.add(egui::DragValue::new(&mut c.target.x).speed(0.1)).changed();
                edited |= ui.add(egui::DragValue::new(&mut c.target.y).speed(0.1)).changed();
                edited |= ui.add(egui::DragValue::new(&mut c.target.z).speed(0.1)).changed();
                ui.end_row();
                ui.label("Angle (°)");
                edited |= ui.add(egui::DragValue::new(&mut rot.x).speed(0.5)).changed();
                edited |= ui.add(egui::DragValue::new(&mut rot.y).speed(0.5)).changed();
                edited |= ui.add(egui::DragValue::new(&mut rot.z).speed(0.5)).changed();
                ui.end_row();
            });
            c.rot = rot * (std::f32::consts::PI / 180.0);
            edited |= ui.add(egui::Slider::new(&mut c.distance, -400.0..=-1.0).text("Distance")).changed();
            edited |= ui.add(egui::Slider::new(&mut c.fov, 5.0..=120.0).text("Field of view")).changed();
            edited |= ui.checkbox(&mut c.ortho, "Orthographic").changed();
        });
        if edited {
            self.scene.camera_edited = true;
        }
        theme::section(ui, "Light");
        theme::card(ui, |ui| {
            let l = &mut self.scene.light;
            let mut col = l.color.to_array();
            let mut ch = false;
            ui.horizontal(|ui| {
                ui.label("Colour");
                ch |= ui.color_edit_button_rgb(&mut col).changed();
            });
            l.color = Vec3::from(col);
            ui.label(RichText::new("Direction").size(11.0).color(theme::TEXT_DIM));
            ui.horizontal(|ui| {
                ch |= ui.add(egui::DragValue::new(&mut l.direction.x).speed(0.01).range(-1.0..=1.0)).changed();
                ch |= ui.add(egui::DragValue::new(&mut l.direction.y).speed(0.01).range(-1.0..=1.0)).changed();
                ch |= ui.add(egui::DragValue::new(&mut l.direction.z).speed(0.01).range(-1.0..=1.0)).changed();
            });
            if ch {
                self.scene.light_edited = true;
            }
        });
        theme::section(ui, "Stage");
        theme::card(ui, |ui| {
            let st = &mut self.scene.stage;
            ui.horizontal(|ui| {
                ui.label("Background");
                ui.color_edit_button_rgb(&mut st.background);
            });
            ui.add(egui::Slider::new(&mut st.shadow_distance, 10.0..=300.0).text("Shadow range"));
        });
        if ui.add(egui::Button::new(RichText::new("◆ Key camera & light  ⏎").color(Color32::BLACK)).fill(theme::KEY)).clicked() {
            self.register_selected();
        }
        ui.label(RichText::new("Camera keys are saved with the camera motion (File → Export camera & light motion).").size(11.0).color(theme::TEXT_DIM));
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
