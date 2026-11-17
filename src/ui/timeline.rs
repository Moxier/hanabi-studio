//! Timeline: transport controls, dope sheet with keyframe selection/moving, audio waveform.

use crate::app::App;
use crate::motion::{Curve, TrackId};
use crate::ui::theme;
use egui::{pos2, vec2, Color32, Rect, RichText, Sense, Stroke};

const LABEL_W: f32 = 200.0;
const ROW_H: f32 = 19.0;

enum Row {
    Group { id: String, label: String, tracks: Vec<TrackId> },
    Track { id: TrackId, label: String },
}

impl App {
    fn track_frames(&self, t: &TrackId) -> Vec<u32> {
        match t {
            TrackId::Camera | TrackId::Light => self.scene.camera_motion.track_frames(t),
            _ => self.scene.selected.and_then(|mi| self.scene.models.get(mi)).map(|m| m.motion.track_frames(t)).unwrap_or_default(),
        }
    }

    /// Key frames of a track that fall inside [from, to] (binary search).
    fn track_frames_in(&self, t: &TrackId, from: u32, to: u32) -> Vec<u32> {
        match t {
            TrackId::Camera | TrackId::Light => self.scene.camera_motion.track_frames_in(t, from, to),
            _ => self.scene.selected.and_then(|mi| self.scene.models.get(mi)).map(|m| m.motion.track_frames_in(t, from, to)).unwrap_or_default(),
        }
    }

    fn build_rows(&self) -> Vec<Row> {
        let mut rows = Vec::new();
        let cam_id = "stage".to_string();
        rows.push(Row::Group { id: cam_id.clone(), label: "Camera & light".into(), tracks: vec![TrackId::Camera, TrackId::Light] });
        if !self.tl.collapsed.contains(&cam_id) {
            rows.push(Row::Track { id: TrackId::Camera, label: "Camera".into() });
            rows.push(Row::Track { id: TrackId::Light, label: "Light".into() });
        }
        if let Some(m) = self.scene.selected.and_then(|i| self.scene.models.get(i)).filter(|m| !m.is_accessory()) {
            for (fi, fr) in m.data.frames.iter().enumerate() {
                let tracks: Vec<(TrackId, String)> = fr
                    .items
                    .iter()
                    .filter(|(_, i)| *i >= 0)
                    .filter_map(|&(is_morph, i)| {
                        if is_morph {
                            m.data.morphs.get(i as usize).map(|mo| (TrackId::Morph(mo.name.clone()), mo.name.clone()))
                        } else {
                            if !self.scene.stage.show_physics_bones && m.physics_bones.get(i as usize).copied().unwrap_or(false) && !m.motion.bones.contains_key(&m.data.bones[i as usize].name) {
                                return None;
                            }
                            m.data.bones.get(i as usize).map(|b| (TrackId::Bone(b.name.clone()), b.name.clone()))
                        }
                    })
                    .collect();
                if tracks.is_empty() {
                    continue;
                }
                let gid = format!("{}/{fi}", m.id);
                rows.push(Row::Group { id: gid.clone(), label: fr.name.clone(), tracks: tracks.iter().map(|t| t.0.clone()).collect() });
                if !self.tl.collapsed.contains(&gid) {
                    for (t, l) in tracks {
                        rows.push(Row::Track { id: t, label: l });
                    }
                }
            }
        }
        rows
    }

    pub fn timeline(&mut self, ui: &mut egui::Ui) {
        ui.set_min_size(ui.available_size());
        self.transport(ui);
        ui.add_space(2.0);
        let full = ui.available_rect_before_wrap();
        let keys_left = full.left() + LABEL_W;
        let ppf = self.tl.px_per_frame;
        let keys_w = (full.right() - keys_left).max(10.0);
        let visible_frames = keys_w / ppf;
        // Follow the playhead: while playing, page forward; when the frame is changed any other way
        // (frame box, arrow keys, jump to key, AI tools), bring it to the middle of the view.
        let frame_changed = (self.scene.frame - self.tl.last_frame).abs() > 0.01;
        if self.tl.follow && frame_changed && self.tl.drag_from.is_none() {
            let x = (self.scene.frame - self.tl.scroll) * ppf;
            if self.scene.playing {
                if x > keys_w * 0.9 || x < 0.0 {
                    self.tl.scroll = (self.scene.frame - visible_frames * 0.1).max(0.0);
                }
            } else if x > keys_w * 0.95 || x < keys_w * 0.02 {
                self.tl.scroll = (self.scene.frame - visible_frames * 0.5).max(0.0);
            }
        }
        self.tl.last_frame = self.scene.frame;
        // zoom (Ctrl+wheel, around the mouse) / pan (Shift+wheel or sideways wheel)
        if ui.rect_contains_pointer(full) {
            let (scroll, mods, hover) = ui.input(|i| (i.smooth_scroll_delta, i.modifiers, i.pointer.hover_pos()));
            if mods.command && (scroll.y != 0.0 || scroll.x != 0.0) {
                let d = if scroll.y != 0.0 { scroll.y } else { scroll.x };
                let anchor_x = hover.map(|p| p.x.max(keys_left)).unwrap_or(keys_left);
                let anchor_f = (anchor_x - keys_left) / ppf + self.tl.scroll;
                self.tl.px_per_frame = (self.tl.px_per_frame * (d * 0.003).exp()).clamp(0.6, 40.0);
                self.tl.scroll = (anchor_f - (anchor_x - keys_left) / self.tl.px_per_frame).max(0.0);
            } else if scroll.x != 0.0 {
                self.tl.scroll = (self.tl.scroll - scroll.x / ppf).max(0.0);
            }
        }
        let ppf = self.tl.px_per_frame;
        let scroll = self.tl.scroll;
        let fx = move |f: f32| keys_left + (f - scroll) * ppf;
        let xf = move |x: f32| ((x - keys_left) / ppf + scroll).max(0.0);

        // Ruler
        let (ruler, rresp) = ui.allocate_exact_size(vec2(full.width(), 22.0), Sense::click_and_drag());
        let p = ui.painter_at(ruler);
        p.rect_filled(ruler, 0.0, theme::BG0);
        let ruler_keys = Rect::from_min_max(pos2(keys_left, ruler.top()), ruler.max);
        let step = if ppf >= 12.0 { 5 } else if ppf >= 5.0 { 10 } else if ppf >= 2.0 { 30 } else { 60 };
        let first = (scroll as i32 / step) * step;
        let mut f = first;
        while fx(f as f32) < ruler.right() {
            let x = fx(f as f32);
            if x >= keys_left {
                p.line_segment([pos2(x, ruler.bottom() - 7.0), pos2(x, ruler.bottom())], Stroke::new(1.0, theme::TEXT_DIM));
                p.text(pos2(x + 2.0, ruler.top() + 3.0), egui::Align2::LEFT_TOP, f.to_string(), egui::FontId::monospace(10.0), theme::TEXT_DIM);
            }
            f += step;
        }
        // play range shading
        let (rs, re) = (fx(self.scene.play_start as f32).max(keys_left), fx(self.scene.play_end as f32).max(keys_left));
        p.rect_filled(Rect::from_min_max(pos2(rs, ruler.bottom() - 3.0), pos2(re, ruler.bottom())), 0.0, theme::ACCENT2.linear_multiply(0.5));
        p.text(pos2(full.left() + 8.0, ruler.center().y), egui::Align2::LEFT_CENTER, format!("{} selected", self.sel_keys.len()), egui::FontId::proportional(11.0), theme::TEXT_DIM);
        if (rresp.clicked() || rresp.dragged()) && rresp.interact_pointer_pos().map(|p| ruler_keys.contains(p)).unwrap_or(false) {
            if let Some(mp) = rresp.interact_pointer_pos() {
                self.scene.set_frame(xf(mp.x).round());
            }
        }

        // Rows
        let rows = self.build_rows();
        let audio_h = if self.scene.audio.is_some() { 34.0 } else { 0.0 };
        // Rows + audio strip must add up to exactly the space left, or the panel grows every frame.
        let sp = ui.spacing().item_spacing.y;
        let audio_total = if self.scene.audio.is_some() { audio_h - 4.0 + sp } else { 0.0 };
        const BAR_H: f32 = 12.0;
        let rows_h = (ui.available_height() - audio_total - (BAR_H + sp) - 1.0).max(20.0);
        let cur = self.scene.frame.round() as u32;
        let mut click_frame: Option<f32> = None;
        let mut toggle_group: Option<String> = None;
        let mut select: Option<(Vec<(TrackId, u32)>, bool)> = None;
        let mut drag_end: Option<i32> = None;
        let mut select_track: Option<TrackId> = None;
        // Only the keys in view are fetched and drawn; selection lookups use a set.
        let vis_from = xf(keys_left - 8.0).floor().max(0.0) as u32;
        let vis_to = xf(full.right() + 8.0).ceil() as u32 + 1;
        let sel_set: std::collections::HashSet<(TrackId, u32)> = self.sel_keys.iter().cloned().collect();
        egui::ScrollArea::vertical().id_salt("tl_rows").max_height(rows_h).auto_shrink([false, false]).show(ui, |ui| {
            for (ri, row) in rows.iter().enumerate() {
                let (rect, resp) = ui.allocate_exact_size(vec2(full.width(), ROW_H), Sense::click_and_drag());
                let p = ui.painter_at(rect);
                let is_group = matches!(row, Row::Group { .. });
                let bg = if is_group { theme::BG2 } else if ri % 2 == 0 { theme::BG1 } else { Color32::from_rgb(28, 30, 37) };
                p.rect_filled(rect, 0.0, bg);
                let label_rect = Rect::from_min_max(rect.min, pos2(keys_left, rect.bottom()));
                let key_rect = Rect::from_min_max(pos2(keys_left, rect.top()), rect.max);
                let (label, tracks, selected_row) = match row {
                    Row::Group { id, label, tracks } => {
                        let col = if self.tl.collapsed.contains(id) { "+" } else { "−" };
                        (format!("{col} {label}"), tracks.clone(), false)
                    }
                    Row::Track { id, label } => {
                        let sel = match id {
                            TrackId::Bone(n) => self.scene.selected.and_then(|mi| self.scene.models[mi].data.bone_index(n).map(|b| self.is_bone_selected(mi, b))).unwrap_or(false),
                            TrackId::Camera | TrackId::Light => self.sel_camera,
                            TrackId::Morph(n) => self.sel_morph.and_then(|mo| self.scene.selected.map(|mi| self.scene.models[mi].data.morphs.get(mo).map(|m| &m.name == n).unwrap_or(false))).unwrap_or(false),
                        };
                        (format!("    {label}"), vec![id.clone()], sel)
                    }
                };
                if selected_row {
                    p.rect_filled(label_rect, 0.0, theme::ACCENT.linear_multiply(0.25));
                }
                p.text(
                    label_rect.left_center() + vec2(6.0, 0.0),
                    egui::Align2::LEFT_CENTER,
                    label,
                    if is_group { egui::FontId::proportional(12.0) } else { egui::FontId::proportional(11.5) },
                    if is_group { Color32::WHITE } else { Color32::from_gray(200) },
                );
                p.line_segment([pos2(keys_left, rect.top()), pos2(keys_left, rect.bottom())], Stroke::new(1.0, theme::BG3));
                // keys
                let mut frames: Vec<u32> = tracks.iter().flat_map(|t| self.track_frames_in(t, vis_from, vis_to)).collect();
                if tracks.len() > 1 {
                    frames.sort_unstable();
                    frames.dedup();
                }
                let pk = ui.painter_at(key_rect);
                let mut last_x = f32::MIN;
                for &f in &frames {
                    let x = fx(f as f32);
                    if x < keys_left - 6.0 || x > rect.right() + 6.0 {
                        continue;
                    }
                    let is_sel = !sel_set.is_empty() && tracks.iter().any(|t| sel_set.contains(&(t.clone(), f)));
                    // when zoomed out, keys closer than a few pixels are drawn once
                    if !is_sel && x - last_x < 3.0 {
                        continue;
                    }
                    last_x = x;
                    let shift = if is_sel && self.tl.drag_from.is_some() { self.tl.drag_delta as f32 * ppf } else { 0.0 };
                    let c = pos2(x + shift, rect.center().y);
                    let s = if is_group { 4.0 } else { 5.0 };
                    let col = if is_sel { theme::ACCENT } else if is_group { theme::KEY.linear_multiply(0.6) } else { theme::KEY };
                    pk.add(egui::Shape::convex_polygon(
                        vec![c + vec2(0.0, -s), c + vec2(s, 0.0), c + vec2(0.0, s), c + vec2(-s, 0.0)],
                        col,
                        Stroke::new(1.0, Color32::from_black_alpha(160)),
                    ));
                }
                // interaction
                let mp = resp.interact_pointer_pos();
                if resp.clicked() {
                    if let Some(mp) = mp {
                        if label_rect.contains(mp) {
                            match row {
                                Row::Group { id, .. } => toggle_group = Some(id.clone()),
                                Row::Track { id, .. } => select_track = Some(id.clone()),
                            }
                        } else {
                            let hit = frames.iter().copied().filter(|&f| (fx(f as f32) - mp.x).abs() < 6.0).min_by_key(|&f| ((fx(f as f32) - mp.x).abs() * 10.0) as i32);
                            let add = ui.input(|i| i.modifiers.shift || i.modifiers.command);
                            match hit {
                                Some(f) => select = Some((tracks.iter().map(|t| (t.clone(), f)).collect(), add)),
                                None => {
                                    click_frame = Some(xf(mp.x).round());
                                    if !add {
                                        select = Some((vec![], false));
                                    }
                                }
                            }
                            if let Row::Track { id, .. } = row {
                                select_track = Some(id.clone());
                            }
                        }
                    }
                }
                if resp.double_clicked() {
                    if let Some(mp) = mp {
                        click_frame = Some(xf(mp.x).round());
                    }
                }
                if resp.drag_started_by(egui::PointerButton::Primary) {
                    if let Some(mp) = ui.input(|i| i.pointer.press_origin()) {
                        if key_rect.contains(mp) {
                            let hit = frames.iter().copied().find(|&f| (fx(f as f32) - mp.x).abs() < 6.0);
                            match hit {
                                Some(f) => {
                                    let already = tracks.iter().any(|t| self.sel_keys.contains(&(t.clone(), f)));
                                    if !already {
                                        self.sel_keys = tracks.iter().map(|t| (t.clone(), f)).collect();
                                    }
                                    self.tl.drag_from = Some((mp.x, f));
                                    self.tl.drag_delta = 0;
                                }
                                None => self.tl.box_start = Some(mp),
                            }
                        }
                    }
                }
                if resp.dragged() {
                    if let (Some((x0, _)), Some(mp)) = (self.tl.drag_from, ui.input(|i| i.pointer.hover_pos())) {
                        self.tl.drag_delta = ((mp.x - x0) / ppf).round() as i32;
                    }
                    if let (Some(b0), Some(mp)) = (self.tl.box_start, ui.input(|i| i.pointer.hover_pos())) {
                        let r = Rect::from_two_pos(pos2(b0.x, rect.top()), pos2(mp.x, rect.bottom()));
                        ui.painter().rect_filled(r, 0.0, theme::ACCENT2.linear_multiply(0.15));
                    }
                }
                if resp.drag_stopped() {
                    if self.tl.drag_from.is_some() {
                        drag_end = Some(self.tl.drag_delta);
                    }
                    if let (Some(b0), Some(mp)) = (self.tl.box_start.take(), ui.input(|i| i.pointer.hover_pos())) {
                        let (a, b) = (xf(b0.x.min(mp.x)), xf(b0.x.max(mp.x)));
                        let picked: Vec<(TrackId, u32)> = tracks
                            .iter()
                            .flat_map(|t| self.track_frames(t).into_iter().filter(|&f| f as f32 >= a && f as f32 <= b).map(move |f| (t.clone(), f)))
                            .collect();
                        select = Some((picked, ui.input(|i| i.modifiers.shift)));
                    }
                }
            }
            // playhead over rows
            let x = fx(self.scene.frame);
            let clip = ui.clip_rect();
            if x >= keys_left {
                ui.painter_at(clip).line_segment([pos2(x, clip.top()), pos2(x, clip.bottom())], Stroke::new(1.5, theme::ACCENT));
            }
        });
        // playhead on ruler
        let x = fx(self.scene.frame);
        if x >= keys_left {
            let p = ui.painter_at(ruler);
            p.add(egui::Shape::convex_polygon(vec![pos2(x - 6.0, ruler.top()), pos2(x + 6.0, ruler.top()), pos2(x, ruler.top() + 9.0)], theme::ACCENT, Stroke::NONE));
            p.line_segment([pos2(x, ruler.top()), pos2(x, ruler.bottom())], Stroke::new(1.5, theme::ACCENT));
            p.text(pos2(x + 8.0, ruler.top() + 3.0), egui::Align2::LEFT_TOP, format!("{cur}"), egui::FontId::monospace(10.5), theme::ACCENT);
        }
        // audio waveform
        if let Some(a) = &self.scene.audio {
            let (r, aresp) = ui.allocate_exact_size(vec2(full.width(), audio_h - 4.0), Sense::click_and_drag());
            let p = ui.painter_at(r);
            p.rect_filled(r, 0.0, theme::BG0);
            p.text(r.left_center() + vec2(8.0, 0.0), egui::Align2::LEFT_CENTER, "♪ Audio", egui::FontId::proportional(12.0), theme::ACCENT2);
            let fps = self.scene.fps;
            let kr = Rect::from_min_max(pos2(keys_left, r.top()), r.max);
            let pa = ui.painter_at(kr);
            let mut x = keys_left;
            while x < r.right() {
                let f = xf(x) - a.offset_frames as f32;
                let idx = (f / fps * 100.0) as isize;
                if idx >= 0 && (idx as usize) < a.info.peaks.len() {
                    let v = a.info.peaks[idx as usize] * (r.height() * 0.48);
                    pa.line_segment([pos2(x, r.center().y - v), pos2(x, r.center().y + v)], Stroke::new(1.0, theme::ACCENT2.linear_multiply(0.8)));
                }
                x += 1.5;
            }
            if aresp.clicked() {
                if let Some(mp) = aresp.interact_pointer_pos() {
                    click_frame = Some(xf(mp.x).round());
                }
            }
        }

        // Horizontal scroll bar
        {
            let visible = keys_w / ppf;
            let total = (self.scene.end_frame().max(self.scene.play_end) as f32 + visible * 0.25).max(self.tl.scroll + visible).max(visible);
            let (bar, bresp) = ui.allocate_exact_size(vec2(full.width(), BAR_H), Sense::click_and_drag());
            let track = Rect::from_min_max(pos2(keys_left, bar.top() + 2.0), pos2(bar.right() - 4.0, bar.bottom() - 2.0));
            let p = ui.painter_at(bar);
            p.rect_filled(track, 4.0, theme::BG0);
            let tw = track.width();
            let thumb_w = (visible / total * tw).clamp(24.0, tw);
            let thumb_x = track.left() + (self.tl.scroll / (total - visible).max(1.0)) * (tw - thumb_w);
            let thumb = Rect::from_min_size(pos2(thumb_x.min(track.right() - thumb_w), track.top()), vec2(thumb_w, track.height()));
            let hot = bresp.hovered() || self.tl.bar_drag.is_some();
            p.rect_filled(thumb, 4.0, if hot { Color32::from_gray(140) } else { Color32::from_gray(95) });
            // playhead marker on the bar
            let px = track.left() + self.scene.frame / total * tw;
            p.line_segment([pos2(px, track.top()), pos2(px, track.bottom())], Stroke::new(2.0, theme::ACCENT));
            p.text(pos2(full.left() + 8.0, bar.center().y), egui::Align2::LEFT_CENTER, format!("frames {:.0}–{:.0}", self.tl.scroll, self.tl.scroll + visible), egui::FontId::proportional(10.0), theme::TEXT_DIM);
            let per_px = (total - visible).max(1.0) / (tw - thumb_w).max(1.0);
            if bresp.drag_started() {
                if let Some(mp) = bresp.interact_pointer_pos() {
                    if thumb.contains(mp) {
                        self.tl.bar_drag = Some((mp.x, self.tl.scroll));
                    } else if track.contains(mp) {
                        // jump so the thumb is centred under the pointer
                        self.tl.scroll = (((mp.x - track.left() - thumb_w * 0.5) * per_px).max(0.0)).min((total - visible).max(0.0));
                        self.tl.bar_drag = Some((mp.x, self.tl.scroll));
                    }
                }
            }
            if let (Some((x0, s0)), Some(mp)) = (self.tl.bar_drag, bresp.interact_pointer_pos()) {
                if bresp.dragged() {
                    self.tl.scroll = (s0 + (mp.x - x0) * per_px).clamp(0.0, (total - visible).max(0.0));
                }
            }
            if bresp.drag_stopped() {
                self.tl.bar_drag = None;
            }
            if bresp.clicked() {
                if let Some(mp) = bresp.interact_pointer_pos() {
                    if track.contains(mp) && !thumb.contains(mp) {
                        self.tl.scroll = (((mp.x - track.left() - thumb_w * 0.5) * per_px).max(0.0)).min((total - visible).max(0.0));
                    }
                }
            }
        }

        // Apply deferred actions
        if let Some(g) = toggle_group {
            if !self.tl.collapsed.remove(&g) {
                self.tl.collapsed.insert(g);
            }
        }
        if let Some(t) = select_track {
            match t {
                TrackId::Bone(n) => {
                    self.sel_camera = false;
                    if let Some(b) = self.scene.selected.and_then(|mi| self.scene.models[mi].data.bone_index(&n)) {
                        let mods = ui.input(|i| i.modifiers);
                        self.click_bone(b, mods);
                    }
                }
                TrackId::Morph(n) => {
                    self.sel_camera = false;
                    if let Some(mi) = self.scene.selected {
                        self.sel_morph = self.scene.models[mi].data.morph_index(&n);
                        self.sel_bone = None;
                    }
                }
                _ => {
                    self.sel_camera = true;
                    self.sel_audio = false;
                }
            }
        }
        if let Some((keys, add)) = select {
            if add {
                for k in keys {
                    if let Some(i) = self.sel_keys.iter().position(|x| *x == k) {
                        self.sel_keys.remove(i);
                    } else {
                        self.sel_keys.push(k);
                    }
                }
            } else {
                self.sel_keys = keys;
            }
        }
        if let Some(f) = click_frame {
            self.scene.set_frame(f);
        }
        if let Some(delta) = drag_end {
            self.tl.drag_from = None;
            self.tl.drag_delta = 0;
            if delta != 0 && !self.sel_keys.is_empty() {
                self.checkpoint();
                let mut keys = self.sel_keys.clone();
                keys.sort_by_key(|k| k.1);
                if delta > 0 {
                    keys.reverse();
                }
                let mut moved = Vec::new();
                for (t, f) in keys {
                    let to = (f as i64 + delta as i64).max(0) as u32;
                    match &t {
                        TrackId::Camera | TrackId::Light => self.scene.camera_motion.move_key(&t, f, to),
                        _ => {
                            if let Some(mi) = self.scene.selected {
                                self.scene.models[mi].motion.move_key(&t, f, to);
                            }
                        }
                    }
                    moved.push((t, to));
                }
                self.sel_keys = moved;
                self.set_status(format!("Moved keys by {delta} frame(s)"));
            }
        }
    }

    fn transport(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let big = |s: &str| RichText::new(s).size(15.0);
            if ui.button(big("⏮")).on_hover_text("Go to start (Home)").clicked() {
                self.scene.set_frame(self.scene.play_start as f32);
                self.scene.force_reset_physics();
            }
            if ui.button(big("⏪")).on_hover_text("Previous key (↓)").clicked() {
                self.jump_key(false);
            }
            let playing = self.scene.playing;
            let play = egui::Button::new(big(if playing { "⏸" } else { "▶" }).color(Color32::WHITE)).fill(theme::ACCENT.linear_multiply(0.8)).min_size(vec2(40.0, 22.0));
            if ui.add(play).on_hover_text("Play / pause (Space)").clicked() {
                self.toggle_play();
            }
            if ui.button(big("⏹")).on_hover_text("Stop").clicked() {
                self.stop();
            }
            if ui.button(big("⏩")).on_hover_text("Next key (↑)").clicked() {
                self.jump_key(true);
            }
            if ui.button(big("⏭")).on_hover_text("Go to end").clicked() {
                self.scene.set_frame(self.scene.end_frame() as f32);
            }
            ui.separator();
            let mut f = self.scene.frame.round() as i32;
            ui.label("Frame");
            if ui.add(egui::DragValue::new(&mut f).range(0..=1_000_000)).changed() {
                self.scene.set_frame(f as f32);
            }
            ui.label(RichText::new(format!("{:.2}s", self.scene.frame / self.scene.fps)).monospace().color(theme::TEXT_DIM));
            ui.separator();
            ui.label("Range");
            ui.add(egui::DragValue::new(&mut self.scene.play_start).range(0..=1_000_000));
            ui.label("–");
            let end_max = 1_000_000u32;
            ui.add(egui::DragValue::new(&mut self.scene.play_end).range(self.scene.play_start..=end_max));
            if ui.small_button("Fit").on_hover_text("Range = whole animation").clicked() {
                self.scene.play_start = 0;
                self.scene.play_end = self.scene.end_frame().max(30);
            }
            ui.toggle_value(&mut self.scene.loop_play, "🔁");
            ui.toggle_value(&mut self.tl.follow, "Follow").on_hover_text("Keep the playhead in view when the frame changes");
            ui.toggle_value(&mut self.scene.audio_scrub, "♪ Frame")
                .on_hover_text("Play WAV with frame: while stopped, each frame you move to (arrow keys, clicking or dragging the timeline) plays that frame's bit of the song. Handy for lip-sync and beat timing.");
            egui::ComboBox::from_id_salt("fps")
                .width(92.0)
                .selected_text(format!("Anim {} fps", self.scene.fps))
                .show_ui(ui, |ui| {
                    ui.label(RichText::new("Keyframe rate of the animation. VMD dances and cameras are made at 30 fps: keep 30 for them, or they play faster or slower.\nThe video's frame rate is set separately on the Render page.").size(11.0).color(theme::TEXT_DIM));
                    for v in [24.0, 30.0, 60.0] {
                        let label = if v == 30.0 { "30 fps (VMD standard)".to_string() } else { format!("{v} fps") };
                        ui.selectable_value(&mut self.scene.fps, v, label);
                    }
                })
                .response
                .on_hover_text("Animation frame rate (keyframes per second). The video frame rate is chosen on the Render page.");
            ui.separator();
            if ui.add(egui::Button::new(RichText::new("◆ Key").color(Color32::BLACK)).fill(theme::KEY)).on_hover_text("Register key (Enter)").clicked() {
                self.register_selected();
            }
            if ui.add_enabled(!self.sel_keys.is_empty(), egui::Button::new("🗑")).on_hover_text("Delete selected keys (Del)").clicked() {
                self.delete_selected_keys();
            }
            if ui.button("⧉").on_hover_text("Copy keys / pose (Ctrl+C)").clicked() {
                self.copy_keys_or_pose();
            }
            if ui.button("📋").on_hover_text("Paste at playhead (Ctrl+V)").clicked() {
                self.paste_keys_or_pose(false);
            }
            if ui.button("⇋").on_hover_text("Paste mirrored (Ctrl+Shift+V)").clicked() {
                self.paste_keys_or_pose(true);
            }
            ui.separator();
            ui.label(RichText::new("New key curve").color(theme::TEXT_DIM));
            for (n, c) in Curve::presets() {
                if crate::ui::anim::curve_icon(ui, c, self.default_curve == c, n).clicked() {
                    self.default_curve = c;
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().slider_width = (ui.available_width() - 30.0).clamp(30.0, 110.0);
                if ui.available_width() > 60.0 {
                    ui.add(egui::Slider::new(&mut self.tl.px_per_frame, 0.6..=40.0).logarithmic(true).show_value(false).text("🔍"))
                        .on_hover_text("Timeline zoom (Ctrl+wheel)");
                }
            });
        });
    }
}
