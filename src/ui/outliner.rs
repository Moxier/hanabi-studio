//! Scene panel: Camera & Light, Audio, and Objects. Objects are models, props and stages, which
//! can be put in groups (groups can hold groups, like collections), dragged between groups by
//! their handle, and hidden a whole group at a time.

use crate::app::App;
use crate::ui::home::ConfirmAction;
use crate::ui::theme;
use egui::{vec2, Color32, RichText, Sense, Stroke};
use glam::{Quat, Vec3};

#[derive(Clone, Copy)]
enum Drag {
    Object(u64),
    Group(u64),
}

enum Op {
    Select(usize),
    Object(&'static str, u64),
    MoveObject(u64, Option<u64>),
    NewGroupWith(u64),
    ToggleGroup(u64),
    OpenGroup(u64, bool),
    RenameGroup(u64),
    NewGroup(Option<u64>),
    MoveGroup(u64, Option<u64>),
    Ungroup(u64),
    DeleteGroup(u64),
    RenameDone,
}

fn row_frame(sel: bool) -> egui::Frame {
    egui::Frame::new()
        .fill(if sel { theme::ACCENT.linear_multiply(0.2) } else { theme::BG2 })
        .stroke(Stroke::new(1.0, if sel { theme::ACCENT } else { theme::BG3 }))
        .corner_radius(egui::CornerRadius::same(6))
        .inner_margin(egui::Margin::symmetric(8, 5))
}

/// Six dots to grab an object or group by.
fn handle(ui: &mut egui::Ui) {
    let (r, _) = ui.allocate_exact_size(vec2(10.0, 16.0), Sense::hover());
    for row in 0..3 {
        for col in 0..2 {
            let p = r.left_top() + vec2(2.5 + col as f32 * 5.0, 3.5 + row as f32 * 4.5);
            ui.painter().circle_filled(p, 1.3, theme::TEXT_DIM);
        }
    }
}

impl App {
    /// The list of groups for "Move to…" menus: (id, indented name).
    fn group_choices(&self) -> Vec<(u64, String)> {
        fn walk(app: &App, parent: Option<u64>, depth: usize, out: &mut Vec<(u64, String)>) {
            for g in app.scene.groups.iter().filter(|g| g.parent == parent) {
                out.push((g.id, format!("{}{}", "   ".repeat(depth), g.name)));
                walk(app, Some(g.id), depth + 1, out);
            }
        }
        let mut v = Vec::new();
        walk(self, None, 0, &mut v);
        v
    }

    pub fn scene_panel(&mut self, ui: &mut egui::Ui) {
        let mut ops: Vec<Op> = Vec::new();

        // ── Camera & Light
        theme::section(ui, "Camera & Light");
        let cam_sel = self.sel_camera;
        let r = row_frame(cam_sel)
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(RichText::new("🎥").size(14.0));
                    ui.label(RichText::new("Camera & Light").strong());
                    let keys = self.scene.camera_motion.camera.len() + self.scene.camera_motion.light.len();
                    ui.label(RichText::new(if keys > 0 { format!("{keys} keys") } else { "no keys".into() }).size(10.5).color(theme::TEXT_DIM));
                });
            })
            .response
            .interact(Sense::click());
        if r.on_hover_text("Select to key the camera and the main light").clicked() {
            self.sel_camera = true;
        }

        // ── Audio
        theme::section(ui, "Audio");
        match &self.scene.audio {
            Some(a) => {
                let name = crate::findfile::display_name(a.path.file_name().unwrap_or_default());
                let dur = a.info.duration;
                let off = a.offset_frames;
                let mut remove = false;
                let mut replace = false;
                row_frame(false).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("♪").size(14.0).color(theme::ACCENT2));
                        ui.add(egui::Label::new(RichText::new(&name).strong()).truncate());
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.add(egui::Button::new("×").frame(false)).on_hover_text("Remove the song").clicked() {
                                remove = true;
                            }
                            if ui.add(egui::Button::new("⟳").frame(false)).on_hover_text("Replace with another song").clicked() {
                                replace = true;
                            }
                        });
                    });
                    let fps = self.scene.fps.max(1.0);
                    let playhead = self.scene.frame.round() as i32;
                    ui.label(RichText::new(format!("{dur:.1} s · plays frames {off} – {}", off + (dur * fps).round() as i32)).size(10.5).color(theme::TEXT_DIM));
                    let Some(t) = self.scene.audio.as_mut() else { return };
                    // Volume and mute
                    ui.horizontal(|ui| {
                        let icon = if t.muted || t.volume <= 0.0 { "🔇" } else { "🔊" };
                        if ui.add(egui::Button::new(icon).frame(false)).on_hover_text(if t.muted { "Unmute" } else { "Mute (also leaves the song out of video exports)" }).clicked() {
                            t.muted = !t.muted;
                        }
                        let mut pct = t.volume * 100.0;
                        ui.spacing_mut().slider_width = (ui.available_width() - 60.0).max(60.0);
                        let r = ui.add_enabled(!t.muted, egui::Slider::new(&mut pct, 0.0..=200.0).suffix("%").step_by(1.0).trailing_fill(true))
                            .on_hover_text("Song volume for playback and video export. 100% = as recorded. Double-click to reset.");
                        if r.changed() {
                            t.volume = (pct / 100.0).clamp(0.0, 2.0);
                        }
                        if r.double_clicked() {
                            t.volume = 1.0;
                        }
                    });
                    // Start offset
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Starts at frame").size(11.5));
                        ui.add(egui::DragValue::new(&mut t.offset_frames).range(-1_000_000..=1_000_000).speed(0.25))
                            .on_hover_text("Move the song later (higher) or earlier (lower) against the animation. Negative skips the song's beginning.");
                        ui.label(RichText::new(format!("{:+.2} s", t.offset_frames as f32 / fps)).size(10.5).color(theme::TEXT_DIM));
                    });
                    ui.horizontal(|ui| {
                        for (label, d, tip) in [("−10", -10, "10 frames earlier"), ("−1", -1, "1 frame earlier"), ("+1", 1, "1 frame later"), ("+10", 10, "10 frames later")] {
                            if ui.small_button(label).on_hover_text(tip).clicked() {
                                t.offset_frames += d;
                            }
                        }
                        if ui.small_button("At playhead").on_hover_text("Start the song at the current frame").clicked() {
                            t.offset_frames = playhead;
                        }
                        if ui.add_enabled(t.offset_frames != 0, egui::Button::new("0").small()).on_hover_text("Start the song at frame 0").clicked() {
                            t.offset_frames = 0;
                        }
                    });
                    ui.checkbox(&mut self.scene.audio_scrub, RichText::new("Play WAV with frame").size(11.5))
                        .on_hover_text("While stopped, each frame you move to plays that frame's bit of the song (for lip-sync and beat timing).");
                });
                if remove {
                    self.ask("Remove the song?", &format!("“{name}” will be taken out of the project (the file stays on disk)."), "Remove", ConfirmAction::RemoveAudio);
                }
                if replace {
                    self.load_dialog("audio");
                }
            }
            None => {
                let mut add = false;
                row_frame(false).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("♪").size(14.0).color(theme::TEXT_DIM));
                        ui.label(RichText::new("No song yet").color(theme::TEXT_DIM));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button("＋ Song (.wav)").on_hover_text("Load a .wav song for the timeline").clicked() {
                                add = true;
                            }
                        });
                    });
                });
                if add {
                    self.load_dialog("audio");
                }
            }
        }

        // ── Objects
        ui.horizontal(|ui| {
            theme::section(ui, "Objects");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.small_button("＋ Group").on_hover_text("New empty group. Drag objects onto it by their dotted handle.").clicked() {
                    ops.push(Op::NewGroup(None));
                }
            });
        });
        if self.scene.models.is_empty() && self.scene.groups.is_empty() {
            ui.label(RichText::new("No objects yet. Add a model, or a prop or stage.").size(11.0).color(theme::TEXT_DIM));
        }
        let choices = self.group_choices();
        self.object_tree(ui, None, &choices, &mut ops);
        // While dragging, a strip to take things out of every group.
        if egui::DragAndDrop::has_payload_of_type::<Drag>(ui.ctx()) {
            let (_, dropped) = ui.dnd_drop_zone::<Drag, ()>(egui::Frame::new().inner_margin(6).corner_radius(6), |ui| {
                ui.set_width(ui.available_width());
                ui.label(RichText::new("Drop here to move to the top level").size(11.0).color(theme::TEXT_DIM));
            });
            match dropped.as_deref() {
                Some(Drag::Object(id)) => ops.push(Op::MoveObject(*id, None)),
                Some(Drag::Group(id)) => ops.push(Op::MoveGroup(*id, None)),
                None => {}
            }
        }
        ui.add_space(2.0);
        ui.horizontal(|ui| {
            let w = ((ui.available_width() - 12.0) / 3.0).max(40.0);
            for (k, l, tip) in [
                ("model", "Model", "Load a .pmx / .pmd character or stage"),
                ("accessory", "Prop (.x)", "Load a .x prop or stage"),
                ("motion", "Motion", "Load a .vmd / .hmo motion onto the selected model (camera motions go to the camera)"),
            ] {
                if ui.add(egui::Button::new(RichText::new(format!("＋ {l}")).size(12.5)).min_size(vec2(w, 24.0))).on_hover_text(tip).clicked() {
                    self.load_dialog(k);
                }
            }
        });
        ui.horizontal(|ui| {
            if ui.small_button("＋ Pose (.vpd)").on_hover_text("Apply a saved pose to the selected model").clicked() {
                self.load_dialog("pose");
            }
        });

        for op in ops {
            self.apply_op(op);
        }
    }

    fn object_tree(&mut self, ui: &mut egui::Ui, parent: Option<u64>, choices: &[(u64, String)], ops: &mut Vec<Op>) {
        let groups: Vec<(u64, String, bool, bool)> = self.scene.groups.iter().filter(|g| g.parent == parent).map(|g| (g.id, g.name.clone(), g.visible, g.open)).collect();
        for (gid, name, vis, open) in groups {
            self.group_header(ui, gid, &name, vis, open, choices, ops);
            if open {
                ui.indent(("group_body", gid), |ui| {
                    self.object_tree(ui, Some(gid), choices, ops);
                    if self.scene.group_object_count(gid) == 0 && !self.scene.groups.iter().any(|g| g.parent == Some(gid)) {
                        ui.label(RichText::new("Empty: drag objects here").size(10.5).color(theme::TEXT_DIM));
                    }
                });
            }
        }
        let ids: Vec<usize> = (0..self.scene.models.len()).filter(|&i| self.scene.models[i].group == parent).collect();
        for i in ids {
            self.object_card(ui, i, choices, ops);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn group_header(&mut self, ui: &mut egui::Ui, gid: u64, name: &str, vis: bool, open: bool, choices: &[(u64, String)], ops: &mut Vec<Op>) {
        let count = self.scene.group_object_count(gid);
        let shown = self.scene.group_shown(gid);
        let frame = egui::Frame::new().fill(theme::BG1).stroke(Stroke::new(1.0, theme::BG3)).corner_radius(egui::CornerRadius::same(6)).inner_margin(egui::Margin::symmetric(6, 4));
        let (_, dropped) = ui.dnd_drop_zone::<Drag, ()>(frame, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.dnd_drag_source(ui.make_persistent_id(("drag_group", gid)), Drag::Group(gid), handle).response.on_hover_text("Drag into another group");
                let arrow = if open { "⏷" } else { "⏵" };
                if ui.add(egui::Button::new(RichText::new(arrow).size(11.0)).frame(false)).on_hover_text(if open { "Fold" } else { "Unfold" }).clicked() {
                    ops.push(Op::OpenGroup(gid, !open));
                }
                ui.label(RichText::new("📁").size(13.0).color(theme::KEY));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.menu_button("⋯", |ui| {
                        if ui.button("Rename").clicked() {
                            ops.push(Op::RenameGroup(gid));
                            ui.close();
                        }
                        if ui.button("New group inside").clicked() {
                            ops.push(Op::NewGroup(Some(gid)));
                            ui.close();
                        }
                        ui.menu_button("Move into", |ui| {
                            if ui.button("Top level").clicked() {
                                ops.push(Op::MoveGroup(gid, None));
                                ui.close();
                            }
                            for (cid, cname) in choices {
                                if !self.scene.group_within(*cid, gid) && ui.button(cname).clicked() {
                                    ops.push(Op::MoveGroup(gid, Some(*cid)));
                                    ui.close();
                                }
                            }
                        });
                        ui.separator();
                        if ui.button("Ungroup (keep objects)").clicked() {
                            ops.push(Op::Ungroup(gid));
                            ui.close();
                        }
                        if ui.button(RichText::new("Delete group and its objects…").color(Color32::from_rgb(255, 130, 120))).clicked() {
                            ops.push(Op::DeleteGroup(gid));
                            ui.close();
                        }
                    });
                    let eye = if vis { "👁" } else { "◌" };
                    if ui.add(egui::Button::new(eye).frame(false)).on_hover_text(if vis { "Hide everything in this group" } else { "Show this group" }).clicked() {
                        ops.push(Op::ToggleGroup(gid));
                    }
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        match &mut self.renaming {
                            Some((rid, buf)) if *rid == gid => {
                                let r = ui.add(egui::TextEdit::singleline(buf).desired_width((ui.available_width() - 24.0).max(40.0)));
                                r.request_focus();
                                if r.lost_focus() || ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                                    ops.push(Op::RenameDone);
                                }
                            }
                            _ => {
                                let r = ui.add(egui::Label::new(RichText::new(name).strong().color(if shown { Color32::WHITE } else { theme::TEXT_DIM })).truncate().sense(Sense::click()));
                                if r.double_clicked() {
                                    ops.push(Op::RenameGroup(gid));
                                } else if r.clicked() {
                                    ops.push(Op::OpenGroup(gid, !open));
                                }
                                r.on_hover_text("Click to fold, double-click to rename");
                            }
                        }
                        ui.label(RichText::new(format!("{count}")).size(10.5).color(theme::TEXT_DIM));
                    });
                });
            });
        });
        match dropped.as_deref() {
            Some(Drag::Object(id)) => ops.push(Op::MoveObject(*id, Some(gid))),
            Some(Drag::Group(id)) if *id != gid => ops.push(Op::MoveGroup(*id, Some(gid))),
            _ => {}
        }
        ui.add_space(3.0);
    }

    fn object_card(&mut self, ui: &mut egui::Ui, i: usize, choices: &[(u64, String)], ops: &mut Vec<Op>) {
        let sel = self.scene.selected == Some(i) && !self.sel_camera;
        let (id, is_acc, vis, shown, name, nb, nm, keys, bodies, character, group) = {
            let m = &self.scene.models[i];
            let character = !m.data.rigid_bodies.is_empty() || m.data.bone_index("上半身").is_some() || m.data.bone_index("頭").is_some();
            let (lo, hi) = m.data.bounds();
            let small = (hi - lo).max_element() < 25.0;
            (m.id, m.is_accessory() || (!character && small), m.visible, m.shown(), m.name().to_string(), m.data.bones.len(), m.data.morphs.len(), m.motion.key_count(), m.data.rigid_bodies.len(), character, m.group)
        };
        let card_id = ui.make_persistent_id(("model_card", id));
        let behind = theme::card_behind(ui, card_id);
        let resp = row_frame(sel)
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.dnd_drag_source(ui.make_persistent_id(("drag_obj", id)), Drag::Object(id), handle).response.on_hover_text("Drag onto a group");
                    let (badge, col) = if is_acc {
                        ("PROP", theme::KEY)
                    } else if character {
                        ("MODEL", theme::ACCENT2)
                    } else {
                        ("STAGE", Color32::from_rgb(150, 210, 140))
                    };
                    ui.label(RichText::new(badge).size(9.5).strong().color(col));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.menu_button("⋯", |ui| {
                            if ui.button("Rename").clicked() {
                                ops.push(Op::Object("rename", id));
                                ui.close();
                            }
                            if ui.button("Duplicate").clicked() {
                                ops.push(Op::Object("duplicate", id));
                                ui.close();
                            }
                            if !is_acc && ui.button("Reset pose").clicked() {
                                ops.push(Op::Object("reset", id));
                                ui.close();
                            }
                            ui.separator();
                            ui.menu_button("Move to group", |ui| {
                                if ui.add_enabled(group.is_some(), egui::Button::new("No group")).clicked() {
                                    ops.push(Op::MoveObject(id, None));
                                    ui.close();
                                }
                                for (gid, gname) in choices {
                                    if ui.add_enabled(group != Some(*gid), egui::Button::new(gname)).clicked() {
                                        ops.push(Op::MoveObject(id, Some(*gid)));
                                        ui.close();
                                    }
                                }
                            });
                            if ui.button("New group with this").clicked() {
                                ops.push(Op::NewGroupWith(id));
                                ui.close();
                            }
                            ui.separator();
                            if ui.button(RichText::new("Delete…").color(Color32::from_rgb(255, 130, 120))).clicked() {
                                ops.push(Op::Object("delete", id));
                                ui.close();
                            }
                        });
                        let eye = if vis { "👁" } else { "◌" };
                        if ui.add(egui::Button::new(eye).frame(false)).on_hover_text(if vis { "Hide" } else { "Show" }).clicked() {
                            ops.push(Op::Object("toggle", id));
                        }
                        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| match &mut self.renaming {
                            Some((rid, buf)) if *rid == id => {
                                let r = ui.add(egui::TextEdit::singleline(buf).desired_width(ui.available_width()));
                                r.request_focus();
                                if r.lost_focus() || ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                                    ops.push(Op::RenameDone);
                                }
                            }
                            _ => {
                                ui.add(egui::Label::new(RichText::new(&name).strong().color(if shown { Color32::WHITE } else { theme::TEXT_DIM })).truncate()).on_hover_text(&name);
                            }
                        });
                    });
                });
                let mut info = if is_acc || !character {
                    format!("{keys} keys")
                } else {
                    format!("{nb} bones · {nm} morphs · {keys} keys{}", if bodies > 0 { format!(" · {bodies} physics") } else { String::new() })
                };
                if vis && !shown {
                    info.push_str(" · group hidden");
                }
                ui.label(RichText::new(info).size(10.5).color(theme::TEXT_DIM));
            })
            .response;
        theme::remember_card(ui, card_id, resp.rect);
        if behind.map(|b| b.clicked()).unwrap_or(false) {
            ops.push(Op::Select(i));
        }
        ui.add_space(3.0);
    }

    fn apply_op(&mut self, op: Op) {
        match op {
            Op::Select(i) => {
                if i < self.scene.models.len() {
                    self.scene.selected = Some(i);
                    self.sel_camera = false;
                    if self.sel_bone.map(|b| b >= self.scene.models[i].data.bones.len()).unwrap_or(false) {
                        self.sel_bone = None;
                    }
                }
            }
            Op::RenameDone => {
                if let Some((id, new)) = self.renaming.take() {
                    let new = new.trim().to_string();
                    if !new.is_empty() {
                        if let Some(g) = self.scene.group_index(id) {
                            self.scene.groups[g].name = new;
                        } else if let Some(i) = self.scene.model_by_id(id) {
                            std::sync::Arc::make_mut(&mut self.scene.models[i].data).name = new;
                        }
                    }
                }
            }
            Op::NewGroup(parent) => {
                let n = self.scene.groups.len() + 1;
                let id = self.scene.new_group(&format!("Group {n}"), parent);
                if let Some(p) = parent.and_then(|p| self.scene.group_index(p)) {
                    self.scene.groups[p].open = true;
                }
                self.renaming = Some((id, format!("Group {n}")));
            }
            Op::NewGroupWith(oid) => {
                if let Some(i) = self.scene.model_by_id(oid) {
                    let parent = self.scene.models[i].group;
                    let n = self.scene.groups.len() + 1;
                    let gid = self.scene.new_group(&format!("Group {n}"), parent);
                    self.scene.models[i].group = Some(gid);
                    self.scene.sync_groups();
                    self.renaming = Some((gid, format!("Group {n}")));
                }
            }
            Op::MoveObject(oid, to) => {
                if let Some(i) = self.scene.model_by_id(oid) {
                    self.scene.models[i].group = to;
                    if let Some(g) = to.and_then(|g| self.scene.group_index(g)) {
                        self.scene.groups[g].open = true;
                    }
                    self.scene.sync_groups();
                }
            }
            Op::MoveGroup(gid, to) => {
                if !self.scene.move_group(gid, to) {
                    self.set_error("A group can't go inside itself");
                }
            }
            Op::ToggleGroup(gid) => {
                if let Some(g) = self.scene.group_index(gid) {
                    self.scene.groups[g].visible = !self.scene.groups[g].visible;
                    self.scene.sync_groups();
                }
            }
            Op::OpenGroup(gid, open) => {
                if let Some(g) = self.scene.group_index(gid) {
                    self.scene.groups[g].open = open;
                }
            }
            Op::RenameGroup(gid) => {
                if let Some(g) = self.scene.group_index(gid) {
                    self.renaming = Some((gid, self.scene.groups[g].name.clone()));
                }
            }
            Op::Ungroup(gid) => {
                let name = self.scene.group_index(gid).map(|g| self.scene.groups[g].name.clone()).unwrap_or_default();
                self.scene.delete_group(gid, true);
                self.set_status(format!("Ungrouped “{name}”; its objects moved up one level"));
            }
            Op::DeleteGroup(gid) => {
                let name = self.scene.group_index(gid).map(|g| self.scene.groups[g].name.clone()).unwrap_or_default();
                let n = self.scene.group_object_count(gid);
                if n == 0 {
                    self.scene.delete_group(gid, false);
                } else {
                    self.ask(
                        "Delete the group and its objects?",
                        &format!("“{name}” and the {n} object(s) inside it, with their keyframes, will be removed from the project. Use Ungroup to keep the objects."),
                        "Delete",
                        ConfirmAction::DeleteGroup(gid),
                    );
                }
            }
            Op::Object(what, id) => {
                let Some(i) = self.scene.model_by_id(id) else { return };
                match what {
                    "toggle" => self.scene.models[i].visible = !self.scene.models[i].visible,
                    "rename" => self.renaming = Some((id, self.scene.models[i].name().to_string())),
                    "duplicate" => {
                        let (data, motion, attach, pos, rot, scale, group) = {
                            let m = &self.scene.models[i];
                            ((*m.data).clone(), m.motion.clone(), m.attach, m.acc_pos, m.acc_rot, m.acc_scale, m.group)
                        };
                        let mut data = data;
                        data.name = format!("{} copy", data.name);
                        let ni = self.scene.add_model(data);
                        let m = &mut self.scene.models[ni];
                        m.motion = motion;
                        m.attach = attach;
                        m.acc_pos = pos + Vec3::new(5.0, 0.0, 0.0);
                        m.acc_rot = rot;
                        m.acc_scale = scale;
                        m.group = group;
                        m.world = glam::Mat4::IDENTITY;
                        self.scene.sync_groups();
                        self.set_status("Duplicated (its motion was copied too)");
                    }
                    "reset" => {
                        let m = &mut self.scene.models[i];
                        for b in 0..m.data.bones.len() {
                            m.pose_edit.insert(b, (Vec3::ZERO, Quat::IDENTITY));
                        }
                        m.morph_edit.clear();
                        self.set_status("Pose reset (press Enter to key it, or change frame to undo)");
                    }
                    "delete" => {
                        let name = self.scene.models[i].name().to_string();
                        let keys = self.scene.models[i].motion.key_count();
                        let extra = if keys > 0 { format!(" Its {keys} keyframes will be lost too.") } else { String::new() };
                        self.ask("Delete from the scene?", &format!("“{name}” will be removed from the project.{extra}"), "Delete", ConfirmAction::DeleteModel(id));
                    }
                    _ => {}
                }
            }
        }
    }
}
