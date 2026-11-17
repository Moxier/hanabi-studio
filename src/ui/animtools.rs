//! Professional animation tools: multi-bone selection, pose resets, multiplying and offsetting
//! the pose or keys of the selected bones, and timing tools (insert / delete frames, stretch,
//! reverse). The Tools tab of the Animation workspace is drawn here too.

use crate::app::App;
use crate::motion::*;
use crate::ui::theme;
use egui::{vec2, Color32, RichText};
use glam::{Quat, Vec3};
use std::collections::BTreeSet;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum SelOp {
    All,
    None,
    Invert,
    Mirror,
    Children,
    Parent,
    Group,
    Keyed,
}

/// Which keys a transform tool changes.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// The pose at the playhead (as an unregistered edit, like posing by hand).
    Pose,
    /// Keys in the timeline's play range.
    Range,
    /// Every key of the bone.
    All,
}

/// Settings of the Tools tab (kept between frames).
pub struct ToolParams {
    pub scope: Scope,
    pub pos_mul: Vec3,
    pub rot_mul: f32,
    pub pos_add: Vec3,
    pub rot_add: Vec3,
    pub frames: u32,
    pub stretch: f32,
    pub with_camera: bool,
    pub only_selected: bool,
}

impl Default for ToolParams {
    fn default() -> Self {
        Self { scope: Scope::Pose, pos_mul: Vec3::ONE, rot_mul: 1.0, pos_add: Vec3::ZERO, rot_add: Vec3::ZERO, frames: 10, stretch: 1.0, with_camera: false, only_selected: false }
    }
}

/// Change the frame of every key in a track with `f` (None removes the key), keeping it sorted.
fn remap<T>(v: &mut Vec<T>, get: impl Fn(&T) -> u32, set: impl Fn(&mut T, u32), f: &impl Fn(u32) -> Option<u32>) {
    let mut out: Vec<T> = Vec::with_capacity(v.len());
    for mut k in v.drain(..) {
        if let Some(nf) = f(get(&k)) {
            set(&mut k, nf);
            out.push(k);
        }
    }
    out.sort_by_key(|k| get(k));
    // Two keys landing on one frame: keep the later one.
    let mut dedup: Vec<T> = Vec::with_capacity(out.len());
    for k in out {
        if dedup.last().map(|l| get(l) == get(&k)).unwrap_or(false) {
            dedup.pop();
        }
        dedup.push(k);
    }
    *v = dedup;
}

impl App {
    /// The model whose bones are being edited.
    pub fn anim_model(&self) -> Option<usize> {
        self.scene.selected.filter(|&i| i < self.scene.models.len() && !self.scene.models[i].is_accessory())
    }

    /// Every selected bone (the active one first).
    pub fn selected_bones(&self) -> Vec<usize> {
        let Some(mi) = self.anim_model() else { return Vec::new() };
        let m = &self.scene.models[mi];
        let n = m.data.bones.len();
        let mut v: Vec<usize> = self.sel_bone.into_iter().filter(|&b| b < n).collect();
        if self.sel_bones.0 == m.id {
            for &b in &self.sel_bones.1 {
                if b < n && !v.contains(&b) {
                    v.push(b);
                }
            }
        }
        v
    }

    pub fn is_bone_selected(&self, mi: usize, b: usize) -> bool {
        self.sel_bone == Some(b) || (self.scene.models.get(mi).map(|m| m.id) == Some(self.sel_bones.0) && self.sel_bones.1.contains(&b))
    }

    fn set_selection(&mut self, mi: usize, bones: BTreeSet<usize>, active: Option<usize>) {
        let id = self.scene.models[mi].id;
        let active = active.filter(|b| bones.contains(b)).or_else(|| bones.iter().next().copied());
        self.sel_bone = active;
        self.sel_bones = (id, bones);
        self.sel_camera = false;
        if active.is_some() {
            self.sel_morph = None;
        }
    }

    /// Click on a bone: plain click selects only it, Ctrl toggles it, Shift adds it.
    pub fn click_bone(&mut self, bi: usize, mods: egui::Modifiers) {
        let Some(mi) = self.anim_model() else { return };
        let mut set: BTreeSet<usize> = if mods.command || mods.shift { self.selected_bones().into_iter().collect() } else { BTreeSet::new() };
        if mods.command && set.contains(&bi) {
            set.remove(&bi);
            let active = if self.sel_bone == Some(bi) { set.iter().next().copied() } else { self.sel_bone };
            self.set_selection(mi, set, active);
        } else {
            set.insert(bi);
            self.set_selection(mi, set, Some(bi));
        }
    }

    /// Bones a person would normally pose (shown, operable, not moved by physics).
    fn posable(&self, mi: usize) -> Vec<usize> {
        let m = &self.scene.models[mi];
        (0..m.data.bones.len())
            .filter(|&i| {
                let b = &m.data.bones[i];
                b.visible && b.operable && (b.rotatable || b.movable) && (self.scene.stage.show_physics_bones || !m.physics_bones.get(i).copied().unwrap_or(false))
            })
            .collect()
    }

    pub fn select_op(&mut self, op: SelOp) {
        let Some(mi) = self.anim_model() else { return };
        let cur: BTreeSet<usize> = self.selected_bones().into_iter().collect();
        let d = self.scene.models[mi].data.clone();
        let frame = self.scene.frame.round() as u32;
        let new: BTreeSet<usize> = match op {
            SelOp::All => self.posable(mi).into_iter().collect(),
            SelOp::None => BTreeSet::new(),
            SelOp::Invert => self.posable(mi).into_iter().filter(|b| !cur.contains(b)).collect(),
            SelOp::Mirror => {
                let mut s = cur.clone();
                for &b in &cur {
                    if let Some(i) = crate::model::mirror_name(&d.bones[b].name).and_then(|n| d.bone_index(&n)) {
                        s.insert(i);
                    }
                }
                s
            }
            SelOp::Children => {
                let mut s = cur.clone();
                let mut grew = true;
                while grew {
                    grew = false;
                    for (i, b) in d.bones.iter().enumerate() {
                        if b.parent >= 0 && s.contains(&(b.parent as usize)) && !s.contains(&i) && b.operable {
                            s.insert(i);
                            grew = true;
                        }
                    }
                }
                s
            }
            SelOp::Parent => {
                let mut s = cur.clone();
                for &b in &cur {
                    let p = d.bones[b].parent;
                    if p >= 0 {
                        s.insert(p as usize);
                    }
                }
                s
            }
            SelOp::Group => {
                let mut s = cur.clone();
                if let Some(a) = self.sel_bone {
                    for fr in &d.frames {
                        if fr.items.iter().any(|&(m, i)| !m && i == a as i32) {
                            s.extend(fr.items.iter().filter(|(m, i)| !*m && *i >= 0).map(|(_, i)| *i as usize));
                        }
                    }
                }
                s
            }
            SelOp::Keyed => {
                let m = &self.scene.models[mi];
                (0..d.bones.len()).filter(|&i| m.motion.bone_key_at(&d.bones[i].name, frame)).collect()
            }
        };
        let active = self.sel_bone.filter(|b| new.contains(b));
        let n = new.len();
        self.set_selection(mi, new, active);
        self.set_status(format!("{n} bone(s) selected"));
    }

    /// Reset the position and/or rotation of the selected bones (as an unregistered edit).
    pub fn reset_selected(&mut self, pos: bool, rot: bool) {
        let Some(mi) = self.anim_model() else { return };
        let bones = self.selected_bones();
        if bones.is_empty() {
            return;
        }
        let frame = self.scene.frame.round() as u32;
        let curves = [self.default_curve; 4];
        let auto = self.auto_key;
        if auto {
            self.checkpoint();
        }
        let m = &mut self.scene.models[mi];
        for &b in &bones {
            let (p, r) = m.bone_pose(b);
            let np = if pos { Vec3::ZERO } else { p };
            let nr = if rot { Quat::IDENTITY } else { r };
            m.pose_edit.insert(b, (np, nr));
            m.skel.states[b].anim_pos = np;
            m.skel.states[b].anim_rot = nr;
            if auto {
                m.register_bone_key(b, frame, curves);
            }
        }
        let what = match (pos, rot) {
            (true, true) => "position and rotation",
            (true, false) => "position",
            _ => "rotation",
        };
        self.set_status(format!("Reset the {what} of {} bone(s){}", bones.len(), if auto { "" } else { " — press Enter to key" }));
    }

    /// Multiply and offset the position/rotation of the selected bones, on the pose or on keys.
    pub fn transform_selected(&mut self) {
        let Some(mi) = self.anim_model() else { return };
        let bones = self.selected_bones();
        if bones.is_empty() {
            self.set_error("Select one or more bones first");
            return;
        }
        let t = &self.tool;
        let (pm, rm, pa, ra, scope) = (t.pos_mul, t.rot_mul, t.pos_add, t.rot_add, t.scope);
        let f = |p: Vec3, r: Quat| -> (Vec3, Quat) {
            let (axis, angle) = r.normalize().to_axis_angle();
            let r2 = if axis.is_finite() && angle.abs() > 1e-6 { Quat::from_axis_angle(axis, angle * rm) } else { r };
            (p * pm + pa, (r2 * quat_from_euler_deg(ra)).normalize())
        };
        let (a, b) = (self.scene.play_start, self.scene.play_end);
        let frame = self.scene.frame.round() as u32;
        self.checkpoint();
        let m = &mut self.scene.models[mi];
        let mut n = 0;
        match scope {
            Scope::Pose => {
                for &bi in &bones {
                    let (p, r) = m.bone_pose(bi);
                    let (np, nr) = f(p, r);
                    m.pose_edit.insert(bi, (np, nr));
                    n += 1;
                }
                if self.auto_key {
                    let curves = [self.default_curve; 4];
                    let m = &mut self.scene.models[mi];
                    for &bi in &bones {
                        m.register_bone_key(bi, frame, curves);
                    }
                }
            }
            Scope::Range | Scope::All => {
                for &bi in &bones {
                    let name = m.data.bones[bi].name.clone();
                    if let Some(v) = m.motion.bones.get_mut(&name) {
                        for k in v.iter_mut().filter(|k| scope == Scope::All || (k.frame >= a && k.frame <= b)) {
                            let (np, nr) = f(k.pos, k.rot);
                            k.pos = np;
                            k.rot = nr;
                            n += 1;
                        }
                    }
                }
            }
        }
        self.set_status(match scope {
            Scope::Pose => format!("Changed the pose of {n} bone(s) — press Enter to key"),
            _ => format!("Changed {n} key(s) on {} bone(s)", bones.len()),
        });
    }

    /// Delete the keys of the selected bones at the playhead.
    pub fn delete_selected_bone_keys(&mut self) {
        let Some(mi) = self.anim_model() else { return };
        let bones = self.selected_bones();
        let frame = self.scene.frame.round() as u32;
        self.checkpoint();
        let m = &mut self.scene.models[mi];
        let mut n = 0;
        for bi in bones {
            let name = m.data.bones[bi].name.clone();
            n += m.motion.delete_key(&TrackId::Bone(name), frame) as usize;
        }
        self.set_status(format!("Deleted {n} key(s) at frame {frame}"));
    }

    /// Apply a frame remapping to the tracks the timing tools work on.
    pub fn retime(&mut self, label: &str, f: impl Fn(u32) -> Option<u32>) {
        let Some(mi) = self.anim_model() else { return };
        self.checkpoint();
        let only: Option<Vec<String>> = self.tool.only_selected.then(|| self.selected_bones().iter().map(|&b| self.scene.models[mi].data.bones[b].name.clone()).collect());
        let m = &mut self.scene.models[mi];
        for (name, v) in m.motion.bones.iter_mut() {
            if only.as_ref().map(|o| o.contains(name)).unwrap_or(true) {
                remap(v, |k| k.frame, |k, x| k.frame = x, &f);
            }
        }
        if only.is_none() {
            for v in m.motion.morphs.values_mut() {
                remap(v, |k| k.frame, |k, x| k.frame = x, &f);
            }
            remap(&mut m.motion.show_ik, |k| k.frame, |k, x| k.frame = x, &f);
        }
        if self.tool.with_camera {
            let c = &mut self.scene.camera_motion;
            remap(&mut c.camera, |k| k.frame, |k, x| k.frame = x, &f);
            remap(&mut c.light, |k| k.frame, |k, x| k.frame = x, &f);
        }
        self.scene.force_reset_physics();
        self.set_status(label.to_string());
    }

    pub fn tools_panel(&mut self, ui: &mut egui::Ui) {
        let Some(mi) = self.anim_model() else {
            ui.label(RichText::new("Select a model to use the animation tools.").color(theme::TEXT_DIM));
            return;
        };
        let nsel = self.selected_bones().len();
        // ── Selection
        theme::section(ui, "Bone selection");
        theme::card(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new(format!("{nsel} selected · Ctrl+click toggles, Shift+click adds (list or view)")).size(10.5).color(theme::TEXT_DIM));
            ui.horizontal_wrapped(|ui| {
                for (op, label, tip) in [
                    (SelOp::All, "All", "Every bone you can pose (A)"),
                    (SelOp::None, "None", "Clear the selection (Alt+A)"),
                    (SelOp::Invert, "Invert", "Swap selected and unselected (Ctrl+I)"),
                    (SelOp::Mirror, "+ Mirror", "Add the other side's bones (left ↔ right)"),
                    (SelOp::Children, "+ Children", "Add every bone below the selected ones"),
                    (SelOp::Parent, "+ Parent", "Add the parent of each selected bone"),
                    (SelOp::Group, "+ Group", "Add the bones in the same group as the active bone"),
                    (SelOp::Keyed, "Keyed here", "Bones with a key at the playhead"),
                ] {
                    if ui.small_button(label).on_hover_text(tip).clicked() {
                        self.select_op(op);
                    }
                }
            });
        });
        // ── Pose
        theme::section(ui, "Pose");
        theme::card(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                if ui.add(egui::Button::new(RichText::new("◆ Key selected  ⏎").color(Color32::BLACK)).fill(theme::KEY)).clicked() {
                    self.register_selected();
                }
                if ui.button("Key all bones").clicked() {
                    self.key_all_bones();
                }
                if ui.add_enabled(nsel > 0, egui::Button::new("Delete keys here")).on_hover_text("Delete the selected bones' keys at the playhead").clicked() {
                    self.delete_selected_bone_keys();
                }
            });
            ui.horizontal_wrapped(|ui| {
                if ui.add_enabled(nsel > 0, egui::Button::new("Reset rotation")).on_hover_text("Selected bones (Alt+R)").clicked() {
                    self.reset_selected(false, true);
                }
                if ui.add_enabled(nsel > 0, egui::Button::new("Reset position")).on_hover_text("Selected bones (Alt+G)").clicked() {
                    self.reset_selected(true, false);
                }
                if ui.button("Reset whole pose").clicked() {
                    self.reset_pose(false);
                }
            });
            ui.horizontal_wrapped(|ui| {
                if ui.button("⇋ Mirror pose").on_hover_text("Swap left and right").clicked() {
                    self.mirror_current_pose();
                }
                if ui.button("Copy pose").on_hover_text("Ctrl+C with no keys selected").clicked() {
                    self.sel_keys.clear();
                    self.copy_keys_or_pose();
                }
                if ui.add_enabled(self.pose_clip.is_some(), egui::Button::new("Paste")).clicked() {
                    self.paste_keys_or_pose(false);
                }
                if ui.add_enabled(self.pose_clip.is_some(), egui::Button::new("Paste mirrored")).on_hover_text("Ctrl+Shift+V").clicked() {
                    self.paste_keys_or_pose(true);
                }
            });
        });
        // ── Multiply / offset
        theme::section(ui, "Multiply & offset");
        theme::card(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                ui.label("Apply to");
                let t = &mut self.tool;
                ui.selectable_value(&mut t.scope, Scope::Pose, "Pose here").on_hover_text("The current pose of the selected bones");
                ui.selectable_value(&mut t.scope, Scope::Range, "Keys in range").on_hover_text(format!("Their keys from frame {} to {}", self.scene.play_start, self.scene.play_end));
                ui.selectable_value(&mut t.scope, Scope::All, "All keys");
            });
            let t = &mut self.tool;
            egui::Grid::new("xform").num_columns(2).spacing(vec2(8.0, 4.0)).show(ui, |ui| {
                ui.label("Position ×").on_hover_text("Scale how far bones move (e.g. 0.5 = half the travel). Double-click a value to reset it.");
                ui.horizontal(|ui| {
                    for (k, c) in [theme::AXIS_X, theme::AXIS_Y, theme::AXIS_Z].into_iter().enumerate() {
                        let r = ui.add(egui::DragValue::new(&mut t.pos_mul[k]).speed(0.01).range(-10.0..=10.0).max_decimals(3)).on_hover_text(["X", "Y", "Z"][k]);
                        theme::dbl(r, &mut t.pos_mul[k], 1.0);
                        let _ = c;
                    }
                });
                ui.end_row();
                ui.label("Rotation ×").on_hover_text("Scale how far bones turn (e.g. 1.2 = 20% bigger moves)");
                let r = ui.add(egui::DragValue::new(&mut t.rot_mul).speed(0.01).range(-5.0..=5.0).max_decimals(3));
                theme::dbl(r, &mut t.rot_mul, 1.0);
                ui.end_row();
                ui.label("Position +");
                ui.horizontal(|ui| {
                    for k in 0..3 {
                        let r = ui.add(egui::DragValue::new(&mut t.pos_add[k]).speed(0.02).max_decimals(3));
                        theme::dbl(r, &mut t.pos_add[k], 0.0);
                    }
                });
                ui.end_row();
                ui.label("Rotation + (°)");
                ui.horizontal(|ui| {
                    for k in 0..3 {
                        let r = ui.add(egui::DragValue::new(&mut t.rot_add[k]).speed(0.5).max_decimals(2));
                        theme::dbl(r, &mut t.rot_add[k], 0.0);
                    }
                });
                ui.end_row();
            });
            ui.horizontal(|ui| {
                if ui.add_enabled(nsel > 0, egui::Button::new("Apply to selected bones")).clicked() {
                    self.transform_selected();
                }
                if ui.small_button("Clear").on_hover_text("Back to ×1 and +0").clicked() {
                    let s = self.tool.scope;
                    self.tool = Default::default();
                    self.tool.scope = s;
                }
            });
        });
        // ── Timing
        theme::section(ui, "Timing");
        theme::card(ui, |ui| {
            ui.set_width(ui.available_width());
            let ph = self.scene.frame.round() as u32;
            let (a, b) = (self.scene.play_start, self.scene.play_end);
            ui.horizontal_wrapped(|ui| {
                ui.checkbox(&mut self.tool.only_selected, "Selected bones only");
                ui.checkbox(&mut self.tool.with_camera, "Include camera & light");
            });
            ui.horizontal_wrapped(|ui| {
                ui.add(egui::DragValue::new(&mut self.tool.frames).range(1..=10000).suffix(" frames"));
                let n = self.tool.frames;
                if ui.button("Insert at playhead").on_hover_text("Push every key at or after the playhead later").clicked() {
                    self.retime(&format!("Inserted {n} frames at {ph}"), move |f| Some(if f >= ph { f + n } else { f }));
                }
                if ui.button("Delete at playhead").on_hover_text("Remove the keys in these frames and pull later keys earlier").clicked() {
                    self.retime(&format!("Deleted {n} frames at {ph}"), move |f| if f < ph { Some(f) } else if f < ph + n { None } else { Some(f - n) });
                }
            });
            ui.horizontal_wrapped(|ui| {
                let r = ui.add(egui::DragValue::new(&mut self.tool.stretch).speed(0.01).range(0.05..=20.0).prefix("× ").max_decimals(3));
                theme::dbl(r, &mut self.tool.stretch, 1.0);
                let s = self.tool.stretch;
                if ui.button(format!("Stretch range {a}–{b}")).on_hover_text("Make the play range slower (>1) or faster (<1); later keys move to follow").clicked() && b > a {
                    let shift = ((b - a) as f32 * (s - 1.0)).round() as i64;
                    self.retime(&format!("Stretched frames {a}–{b} ×{s}"), move |f| {
                        let nf = if f < a {
                            f as i64
                        } else if f <= b {
                            a as i64 + ((f - a) as f32 * s).round() as i64
                        } else {
                            f as i64 + shift
                        };
                        Some(nf.max(0) as u32)
                    });
                }
            });
            if ui.button(format!("Reverse range {a}–{b}")).on_hover_text("Play the keys in the range backwards").clicked() && b > a {
                self.retime(&format!("Reversed frames {a}–{b}"), move |f| Some(if f >= a && f <= b { a + b - f } else { f }));
            }
            if ui.add_enabled(nsel > 0, egui::Button::new("Smooth selected bones' keys")).on_hover_text("Softens jittery captured motion").clicked() {
                self.checkpoint();
                let names: Vec<String> = self.selected_bones().iter().map(|&b| self.scene.models[mi].data.bones[b].name.clone()).collect();
                for name in &names {
                    if let Some(v) = self.scene.models[mi].motion.bones.get_mut(name) {
                        crate::tools::smooth_track(v, 0.8);
                    }
                }
                self.set_status(format!("Smoothed the keys of {} bone(s)", names.len()));
            }
        });
    }
}
