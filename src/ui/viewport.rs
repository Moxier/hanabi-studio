//! The 3D viewport: image, camera navigation, bone overlay, picking and the transform gizmo.

use crate::app::{App, GizmoDrag, GizmoMode};
use crate::ui::theme;
use egui::{pos2, vec2, Color32, Pos2, Rect, RichText, Sense, Stroke};
use glam::{Mat4, Quat, Vec3};

pub struct Projector {
    pub vp: Mat4,
    pub rect: Rect,
}

impl Projector {
    pub fn project(&self, p: Vec3) -> Option<(Pos2, f32)> {
        let c = self.vp * p.extend(1.0);
        if c.w <= 0.05 {
            return None;
        }
        let n = c.truncate() / c.w;
        Some((pos2(self.rect.left() + (n.x * 0.5 + 0.5) * self.rect.width(), self.rect.top() + (0.5 - n.y * 0.5) * self.rect.height()), c.w))
    }
}

fn dist_to_segment(p: Pos2, a: Pos2, b: Pos2) -> f32 {
    let ab = b - a;
    let t = ((p - a).dot(ab) / ab.length_sq().max(1e-6)).clamp(0.0, 1.0);
    (a + ab * t - p).length()
}

impl App {
    /// Draw the 3D view in `rect`. `tools` enables bone picking and gizmos.
    pub fn viewport(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame, full_rect: Rect, tools: bool) {
        // Output framing: where the final render's picture sits inside the viewport.
        let out_aspect = self.render_settings.width as f32 / self.render_settings.height.max(1) as f32;
        let frame_rect = {
            let (mut w, mut h) = (full_rect.width(), full_rect.width() / out_aspect);
            if h > full_rect.height() {
                h = full_rect.height();
                w = h * out_aspect;
            }
            Rect::from_center_size(full_rect.center(), vec2(w.max(8.0), h.max(8.0)))
        };
        let in_render_ws = self.ws == crate::app::Workspace::Render;
        let mode = if in_render_ws { crate::app::FrameMode::Only } else { self.frame_mode };
        let rect = if mode == crate::app::FrameMode::Only { frame_rect } else { full_rect };
        self.view_proj = if mode == crate::app::FrameMode::Guide {
            // The output camera maps the frame rect to the full picture; scale it into the viewport.
            let fx = frame_rect.width() / full_rect.width().max(1.0);
            let fy = frame_rect.height() / full_rect.height().max(1.0);
            Some(Mat4::from_scale(Vec3::new(fx, fy, 1.0)) * self.scene.camera.proj(out_aspect))
        } else {
            None
        };
        if mode == crate::app::FrameMode::Only && !in_render_ws {
            ui.painter().rect_filled(full_rect, 0.0, Color32::from_rgb(10, 10, 13));
        }
        self.viewport_rect = rect;
        let ppp = ui.ctx().pixels_per_point();
        let (w, h) = ((rect.width() * ppp).round() as i32, (rect.height() * ppp).round() as i32);
        let painter = ui.painter_at(rect);
        match self.render_viewport(frame, w.max(16), h.max(16)) {
            Some(tex) => {
                painter.image(tex, rect, Rect::from_min_max(pos2(0.0, 1.0), pos2(1.0, 0.0)), Color32::WHITE);
            }
            None => {
                painter.rect_filled(rect, 0.0, theme::BG0);
                painter.text(rect.center(), egui::Align2::CENTER_CENTER, "3D view unavailable", egui::FontId::proportional(16.0), theme::TEXT_DIM);
            }
        }
        let resp = ui.interact(rect, ui.scope_id().with("viewport"), Sense::click_and_drag());

        // --- camera navigation
        let cam_scale = {
            let c = &self.scene.camera;
            c.distance.abs().max(1.0) * 2.0 * (c.fov.to_radians() * 0.5).tan() / rect.height().max(1.0)
        };
        let inv_view = self.scene.camera.view().inverse();
        let right = inv_view.transform_vector3(Vec3::X).normalize_or_zero();
        let up = inv_view.transform_vector3(Vec3::Y).normalize_or_zero();
        let shift = ui.input(|i| i.modifiers.shift);
        let alt = ui.input(|i| i.modifiers.alt);
        let mut cam_changed = false;
        if resp.dragged_by(egui::PointerButton::Middle) || (resp.dragged_by(egui::PointerButton::Secondary) && shift) {
            let d = resp.drag_delta();
            self.scene.camera.target += (-right * d.x + up * d.y) * cam_scale;
            cam_changed = true;
        } else if resp.dragged_by(egui::PointerButton::Secondary) || (resp.dragged_by(egui::PointerButton::Primary) && alt) {
            let d = resp.drag_delta();
            self.scene.camera.rot.y -= d.x * 0.0075;
            self.scene.camera.rot.x = (self.scene.camera.rot.x + d.y * 0.0075).clamp(-1.55, 1.55);
            cam_changed = true;
        }
        if resp.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll.abs() > 0.0 {
                let d = &mut self.scene.camera.distance;
                *d = (*d * (-scroll * 0.0015).exp()).clamp(-3000.0, -0.5);
                cam_changed = true;
            }
        }
        if cam_changed && !self.scene.camera_motion.camera.is_empty() {
            self.scene.camera_edited = true;
        }

        let aspect = rect.width() / rect.height().max(1.0);
        let pm = self.view_proj.unwrap_or_else(|| self.scene.camera.proj(aspect));
        let proj = Projector { vp: pm * self.scene.camera.view(), rect };
        // Render-area guide
        if mode == crate::app::FrameMode::Guide {
            let dim = Color32::from_black_alpha(140);
            for r in [
                Rect::from_min_max(full_rect.min, pos2(full_rect.right(), frame_rect.top())),
                Rect::from_min_max(pos2(full_rect.left(), frame_rect.bottom()), full_rect.max),
                Rect::from_min_max(pos2(full_rect.left(), frame_rect.top()), pos2(frame_rect.left(), frame_rect.bottom())),
                Rect::from_min_max(pos2(frame_rect.right(), frame_rect.top()), pos2(full_rect.right(), frame_rect.bottom())),
            ] {
                if r.width() > 0.5 && r.height() > 0.5 {
                    painter.rect_filled(r, 0.0, dim);
                }
            }
            painter.rect_stroke(frame_rect, 0.0, Stroke::new(1.0, Color32::from_white_alpha(120)), egui::StrokeKind::Inside);
            // title-safe guide (90%)
            painter.rect_stroke(frame_rect.shrink2(frame_rect.size() * 0.05), 0.0, Stroke::new(1.0, Color32::from_white_alpha(35)), egui::StrokeKind::Inside);
        }
        if mode != crate::app::FrameMode::Full {
            painter.text(
                frame_rect.left_bottom() + vec2(6.0, -6.0),
                egui::Align2::LEFT_BOTTOM,
                format!("Output {}×{}", self.render_settings.width, self.render_settings.height),
                egui::FontId::proportional(11.0),
                Color32::from_white_alpha(170),
            );
        }
        if !in_render_ws {
            let labels = [(crate::app::FrameMode::Full, "Full view"), (crate::app::FrameMode::Guide, "Show render area"), (crate::app::FrameMode::Only, "Render area only")];
            let mut x = full_rect.right() - 8.0;
            for (m, l) in labels.iter().rev() {
                let w = 10.0 + l.len() as f32 * 6.4;
                let r = Rect::from_min_size(pos2(x - w, full_rect.top() + 6.0), vec2(w, 20.0));
                x -= w + 4.0;
                let sel = self.frame_mode == *m;
                let b = egui::Button::new(RichText::new(*l).size(11.0).color(if sel { Color32::WHITE } else { Color32::from_gray(190) }))
                    .fill(if sel { theme::ACCENT.linear_multiply(0.6) } else { Color32::from_black_alpha(140) })
                    .corner_radius(egui::CornerRadius::same(10));
                if ui.put(r, b).on_hover_text("How the view shows the final render's framing (size set in the Render workspace)").clicked() {
                    self.frame_mode = *m;
                }
            }
        }

        // --- overlays
        if tools {
            self.gizmo_and_bones(ui, &resp, &proj, &painter, rect);
        }
        // Physics wireframes: rigid bodies (orange = simulated, blue = following a bone,
        // green = simulated with the bone's position) and joints, with the part selected in the
        // physics panel highlighted.
        let sel = self.phys_sel;
        if self.scene.stage.show_rigid || self.scene.stage.show_joints || sel.is_some() {
            for m in &self.scene.models {
                let Some(ph) = &m.physics else { continue };
                if !m.shown() {
                    continue;
                }
                let picked = sel.filter(|s| s.0 == m.id);
                let line = |a: Vec3, b: Vec3, st: Stroke| {
                    if let (Some((pa, _)), Some((pb, _))) = (proj.project(m.world.transform_point3(a)), proj.project(m.world.transform_point3(b))) {
                        painter.line_segment([pa, pb], st);
                    }
                };
                for (i, t, shape, size, mode) in ph.debug_shapes(&m.data) {
                    let is_sel = picked.map(|s| !s.1 && s.2 == i).unwrap_or(false);
                    if !self.scene.stage.show_rigid && !is_sel {
                        continue;
                    }
                    let col = if is_sel {
                        Color32::WHITE
                    } else {
                        match mode {
                            crate::model::RigidMode::FollowBone => Color32::from_rgba_unmultiplied(90, 200, 255, 120),
                            crate::model::RigidMode::Physics => Color32::from_rgba_unmultiplied(255, 170, 60, 140),
                            crate::model::RigidMode::PhysicsWithBonePosition => Color32::from_rgba_unmultiplied(140, 230, 120, 140),
                        }
                    };
                    let st = Stroke::new(if is_sel { 2.0 } else { 1.0 }, col);
                    let ring = |center: Vec3, ax: Vec3, ay: Vec3, r: f32, from: f32, to: f32| {
                        let n = 16;
                        let mut prev = None;
                        for k in 0..=n {
                            let a = from + (to - from) * k as f32 / n as f32;
                            let p = t.transform_point3(center + (ax * a.cos() + ay * a.sin()) * r);
                            if let Some(q) = prev {
                                line(q, p, st);
                            }
                            prev = Some(p);
                        }
                    };
                    let tau = std::f32::consts::TAU;
                    let pi = std::f32::consts::PI;
                    match shape {
                        crate::model::Shape::Box => {
                            let c: Vec<Vec3> = (0..8).map(|k| t.transform_point3(Vec3::new(if k & 1 == 0 { -size.x } else { size.x }, if k & 2 == 0 { -size.y } else { size.y }, if k & 4 == 0 { -size.z } else { size.z }))).collect();
                            for (a, b) in [(0, 1), (2, 3), (4, 5), (6, 7), (0, 2), (1, 3), (4, 6), (5, 7), (0, 4), (1, 5), (2, 6), (3, 7)] {
                                line(c[a], c[b], st);
                            }
                        }
                        crate::model::Shape::Sphere => {
                            ring(Vec3::ZERO, Vec3::X, Vec3::Y, size.x, 0.0, tau);
                            ring(Vec3::ZERO, Vec3::X, Vec3::Z, size.x, 0.0, tau);
                            ring(Vec3::ZERO, Vec3::Y, Vec3::Z, size.x, 0.0, tau);
                        }
                        crate::model::Shape::Capsule => {
                            let (r, h) = (size.x, size.y * 0.5);
                            ring(Vec3::Y * h, Vec3::X, Vec3::Z, r, 0.0, tau);
                            ring(-Vec3::Y * h, Vec3::X, Vec3::Z, r, 0.0, tau);
                            ring(Vec3::Y * h, Vec3::X, Vec3::Y, r, 0.0, pi);
                            ring(Vec3::Y * h, Vec3::Z, Vec3::Y, r, 0.0, pi);
                            ring(-Vec3::Y * h, Vec3::X, Vec3::Y, r, pi, tau);
                            ring(-Vec3::Y * h, Vec3::Z, Vec3::Y, r, pi, tau);
                            for d in [Vec3::X, -Vec3::X, Vec3::Z, -Vec3::Z] {
                                line(t.transform_point3(d * r + Vec3::Y * h), t.transform_point3(d * r - Vec3::Y * h), st);
                            }
                        }
                    }
                }
                for (ji, at, ca, cb) in ph.debug_joints() {
                    let is_sel = picked.map(|s| s.1 && s.2 == ji).unwrap_or(false);
                    if !self.scene.stage.show_joints && !is_sel {
                        continue;
                    }
                    let col = if is_sel { Color32::WHITE } else { Color32::from_rgba_unmultiplied(255, 90, 200, 170) };
                    let st = Stroke::new(if is_sel { 2.0 } else { 1.0 }, col);
                    line(ca, at, st);
                    line(at, cb, st);
                    if let Some((p, _)) = proj.project(m.world.transform_point3(at)) {
                        painter.circle_filled(p, if is_sel { 4.0 } else { 2.5 }, col);
                    }
                }
            }
        }
        // axis indicator
        let o = rect.left_bottom() + vec2(36.0, -36.0);
        let view = self.scene.camera.view();
        for (axis, col, name) in [(Vec3::X, theme::AXIS_X, "X"), (Vec3::Y, theme::AXIS_Y, "Y"), (Vec3::Z, theme::AXIS_Z, "Z")] {
            let v = view.transform_vector3(axis);
            let end = o + vec2(v.x, -v.y) * 22.0;
            painter.line_segment([o, end], Stroke::new(2.0, col));
            painter.text(end, egui::Align2::CENTER_CENTER, name, egui::FontId::proportional(10.0), col);
        }
        // HUD
        let mut hud = format!("Frame {:.0}", self.scene.frame);
        if self.scene.playing {
            hud.push_str("  ▶");
        }
        if self.scene.models.iter().any(|m| !m.pose_edit.is_empty() || !m.morph_edit.is_empty()) {
            hud.push_str("   ● unregistered pose — press Enter to key");
        }
        painter.text(rect.left_top() + vec2(10.0, 8.0), egui::Align2::LEFT_TOP, hud, egui::FontId::proportional(12.5), Color32::from_white_alpha(200));
        if self.scene.camera_edited {
            painter.text(full_rect.right_top() + vec2(-10.0, 32.0), egui::Align2::RIGHT_TOP, "camera moved (not keyed)", egui::FontId::proportional(11.5), theme::KEY);
        }
        if self.scene.models.is_empty() {
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "Drop a model (.pmx / .pmd) here, or use File → Open",
                egui::FontId::proportional(18.0),
                Color32::from_white_alpha(170),
            );
        }
    }

    fn gizmo_and_bones(&mut self, ui: &mut egui::Ui, resp: &egui::Response, proj: &Projector, painter: &egui::Painter, rect: Rect) {
        let Some(mi) = self.scene.selected else { return };
        if mi >= self.scene.models.len() || self.scene.models[mi].is_accessory() {
            return;
        }
        let pointer = ui.input(|i| i.pointer.hover_pos());
        let model = &self.scene.models[mi];
        let nb = model.data.bones.len();
        let show_phys = self.scene.stage.show_physics_bones;
        let hidden = |i: usize| !show_phys && model.physics_bones.get(i).copied().unwrap_or(false);
        let mut screen: Vec<Option<Pos2>> = vec![None; nb];
        for i in 0..nb {
            let p = model.bone_world(i).w_axis.truncate();
            screen[i] = proj.project(p).map(|x| x.0);
        }
        // Bones
        let mut hover_bone = None;
        if self.scene.stage.show_bones {
            let mut best = 10.0f32;
            for (i, b) in model.data.bones.iter().enumerate() {
                if !b.visible || !b.operable || hidden(i) {
                    continue;
                }
                if let (Some(sp), Some(mp)) = (screen[i], pointer) {
                    let d = (sp - mp).length();
                    if d < best && rect.contains(mp) {
                        best = d;
                        hover_bone = Some(i);
                    }
                }
            }
            for (i, b) in model.data.bones.iter().enumerate() {
                if !b.visible || hidden(i) {
                    continue;
                }
                let Some(sp) = screen[i] else { continue };
                if b.parent >= 0 {
                    let pi = b.parent as usize;
                    if pi < nb && model.data.bones[pi].visible {
                        if let Some(pp) = screen[pi] {
                            painter.line_segment([pp, sp], Stroke::new(1.6, Color32::from_rgba_unmultiplied(140, 190, 255, 150)));
                        }
                    }
                }
            }
            for (i, b) in model.data.bones.iter().enumerate() {
                if !b.visible || !b.operable || hidden(i) {
                    continue;
                }
                let Some(sp) = screen[i] else { continue };
                let sel = self.sel_bone == Some(i);
                let hov = hover_bone == Some(i);
                let keyed = model.motion.bone_key_at(&b.name, self.scene.frame.round() as u32);
                let base = if b.ik.is_some() {
                    Color32::from_rgb(255, 160, 60)
                } else if b.movable {
                    Color32::from_rgb(110, 220, 140)
                } else {
                    Color32::from_rgb(150, 200, 255)
                };
                let col = if sel { theme::ACCENT } else if hov { Color32::WHITE } else { base };
                let r = if sel { 6.0 } else { 4.5 };
                if b.ik.is_some() {
                    painter.rect_filled(Rect::from_center_size(sp, vec2(r * 2.0, r * 2.0)), 1.0, col);
                } else {
                    painter.circle_filled(sp, r, col.linear_multiply(0.85));
                }
                if keyed {
                    painter.circle_stroke(sp, r + 2.5, Stroke::new(1.2, theme::KEY));
                }
            }
        }

        // Gizmo
        let Some(bi) = self.sel_bone.filter(|&b| b < nb) else {
            if resp.clicked() {
                if let Some(h) = hover_bone {
                    self.sel_bone = Some(h);
                }
            }
            return;
        };
        let b = &model.data.bones[bi];
        let world = model.bone_world(bi);
        let center = world.w_axis.truncate();
        let Some((cs, depth)) = proj.project(center) else { return };
        // World units per screen pixel at the bone, measured through the actual projection.
        let cam_right = self.scene.camera.view().inverse().transform_vector3(Vec3::X).normalize_or(Vec3::X);
        let units_px = match proj.project(center + cam_right) {
            Some((p1, _)) => 1.0 / (p1 - cs).length().max(1e-3),
            None => depth * 2.0 * (self.scene.camera.fov.to_radians() * 0.5).tan() / rect.height(),
        };
        let (_, grot, _) = world.to_scale_rotation_translation();
        let axes = if self.gizmo_local {
            [grot * Vec3::X, grot * Vec3::Y, grot * Vec3::Z]
        } else {
            [Vec3::X, Vec3::Y, Vec3::Z]
        };
        let eye = self.scene.camera.eye();
        let view_axis = (eye - center).normalize_or(Vec3::Z);
        let colors = [theme::AXIS_X, theme::AXIS_Y, theme::AXIS_Z, Color32::from_gray(220)];
        let mode = if self.gizmo == GizmoMode::Move && b.movable {
            GizmoMode::Move
        } else if b.rotatable {
            GizmoMode::Rotate
        } else if b.movable {
            GizmoMode::Move
        } else {
            return;
        };
        let radius_px = 64.0;
        let r_world = radius_px * units_px;
        let mut hover_axis = None;
        let mut shapes: Vec<(usize, Vec<Pos2>)> = Vec::new();
        match mode {
            GizmoMode::Rotate => {
                for (ai, axis) in axes.iter().chain(std::iter::once(&view_axis)).enumerate() {
                    let rr = if ai == 3 { r_world * 1.22 } else { r_world };
                    let u = axis.any_orthonormal_vector();
                    let v = axis.cross(u);
                    let pts: Vec<Pos2> = (0..=64)
                        .filter_map(|k| {
                            let a = k as f32 / 64.0 * std::f32::consts::TAU;
                            proj.project(center + (u * a.cos() + v * a.sin()) * rr).map(|x| x.0)
                        })
                        .collect();
                    shapes.push((ai, pts));
                }
            }
            GizmoMode::Move => {
                for (ai, axis) in axes.iter().enumerate() {
                    if let Some((e, _)) = proj.project(center + *axis * r_world) {
                        shapes.push((ai, vec![cs, e]));
                    }
                }
            }
        }
        if let Some(mp) = pointer {
            let mut best = 7.0f32;
            for (ai, pts) in &shapes {
                for w in pts.windows(2) {
                    let d = dist_to_segment(mp, w[0], w[1]);
                    if d < best {
                        best = d;
                        hover_axis = Some(*ai);
                    }
                }
            }
        }
        if self.gizmo_drag.is_none() {
            self.gizmo_hover = hover_axis;
        }
        let active = self.gizmo_drag.as_ref().map(|d| d.axis).or(hover_axis);
        painter.circle_filled(cs, radius_px + 2.0, Color32::from_rgba_unmultiplied(0, 0, 0, 25));
        for (ai, pts) in &shapes {
            let hl = active == Some(*ai);
            let col = if hl { Color32::from_rgb(255, 235, 120) } else { colors[*ai] };
            let width = if hl { 3.2 } else { 2.0 };
            if mode == GizmoMode::Move {
                painter.arrow(pts[0], pts[1] - pts[0], Stroke::new(width + 0.6, col));
            } else {
                painter.add(egui::Shape::line(pts.clone(), Stroke::new(width, col)));
            }
        }
        painter.text(cs + vec2(0.0, -radius_px - 16.0), egui::Align2::CENTER_CENTER, &b.name, egui::FontId::proportional(12.0), Color32::WHITE);

        let parent_rot = if b.parent >= 0 {
            let (_, r, _) = model.bone_world(b.parent as usize).to_scale_rotation_translation();
            r
        } else {
            let (_, r, _) = model.world.to_scale_rotation_translation();
            r
        };
        let (p0, r0) = model.bone_pose(bi);
        // Interaction
        if resp.drag_started_by(egui::PointerButton::Primary) && !ui.input(|i| i.modifiers.alt) {
            if let (Some(ax), Some(mp)) = (hover_axis, pointer) {
                self.checkpoint();
                self.gizmo_drag = Some(GizmoDrag { axis: ax, last: mp });
            }
        }
        if let (Some(drag), Some(mp)) = (self.gizmo_drag.clone(), pointer) {
            if resp.dragged_by(egui::PointerButton::Primary) {
                let axis = if drag.axis == 3 { view_axis } else { axes[drag.axis] };
                match mode {
                    GizmoMode::Rotate => {
                        let a0 = (drag.last - cs).angle();
                        let a1 = (mp - cs).angle();
                        let mut da = a1 - a0;
                        if da > std::f32::consts::PI {
                            da -= std::f32::consts::TAU;
                        }
                        if da < -std::f32::consts::PI {
                            da += std::f32::consts::TAU;
                        }
                        // Which way does +rotation move on screen around this axis?
                        let u = axis.any_orthonormal_vector() * r_world;
                        let q = Quat::from_axis_angle(axis, 0.2);
                        let sign = match (proj.project(center + u), proj.project(center + q * u)) {
                            (Some((s0, _)), Some((s1, _))) => {
                                let mut d = (s1 - cs).angle() - (s0 - cs).angle();
                                if d > std::f32::consts::PI {
                                    d -= std::f32::consts::TAU;
                                }
                                if d < -std::f32::consts::PI {
                                    d += std::f32::consts::TAU;
                                }
                                d.signum()
                            }
                            _ => 1.0,
                        };
                        let delta = Quat::from_axis_angle(axis, da * sign);
                        let local = (parent_rot.inverse() * delta * parent_rot) * r0;
                        self.edit_bone(mi, bi, p0, local.normalize());
                    }
                    GizmoMode::Move => {
                        if let Some((e, _)) = proj.project(center + axis * r_world) {
                            let dir = e - cs;
                            let len = dir.length().max(1.0);
                            let px = (mp - drag.last).dot(dir / len);
                            let world_delta = axis * (px / len) * r_world;
                            let local = parent_rot.inverse() * world_delta;
                            self.edit_bone(mi, bi, p0 + local, r0);
                        }
                    }
                }
                self.gizmo_drag = Some(GizmoDrag { axis: drag.axis, last: mp });
            }
        }
        if resp.drag_stopped() {
            if self.gizmo_drag.take().is_some() && self.auto_key {
                self.register_selected();
            }
        }
        if resp.clicked() && hover_axis.is_none() {
            if let Some(h) = hover_bone {
                self.sel_bone = Some(h);
            }
        }
        let _ = RichText::new("");
    }
}
