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
    LoadAnimation,
}

const MODEL_COLOR: Color32 = Color32::from_rgb(86, 170, 110);
const STAGE_COLOR: Color32 = Color32::from_rgb(70, 150, 160);
const PROP_COLOR: Color32 = Color32::from_rgb(200, 140, 60);
const AUDIO_COLOR: Color32 = Color32::from_rgb(196, 110, 200);
const CAM_COLOR: Color32 = Color32::from_rgb(90, 130, 220);

/// A card like the sketch: a coloured header bar (drag handle, fold arrow, type on the right)
/// over the body. Clicking anywhere on the card that isn't a button returns `clicked`; the fold
/// arrow returns `toggled`. Text inside never catches the click.
#[allow(clippy::too_many_arguments)]
fn card_shell<R>(ui: &mut egui::Ui, id: egui::Id, color: Color32, type_label: &str, title: &str, selected: bool, collapsed: bool, drag: Option<(egui::Id, Drag)>, body: impl FnOnce(&mut egui::Ui) -> R) -> (bool, bool) {
    let behind = theme::card_behind(ui, id);
    let hovered = behind.as_ref().map(|b| b.hovered()).unwrap_or(false);
    let mut toggled = false;
    let outer = egui::Frame::new()
        .fill(theme::BG2)
        .stroke(Stroke::new(if selected { 2.0 } else { 1.0 }, if selected { theme::ACCENT } else if hovered { theme::BG3.gamma_multiply(1.6) } else { theme::BG3 }))
        .corner_radius(egui::CornerRadius::same(7))
        .inner_margin(egui::Margin::same(0))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 3.0;
            // header bar
            egui::Frame::new()
                .fill(color.linear_multiply(if selected { 0.75 } else { 0.5 }))
                .corner_radius(egui::CornerRadius { nw: 7, ne: 7, sw: if collapsed { 7 } else { 0 }, se: if collapsed { 7 } else { 0 } })
                .inner_margin(egui::Margin::symmetric(6, 3))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        if let Some((did, payload)) = drag {
                            ui.dnd_drag_source(did, payload, handle).response.on_hover_text("Drag onto a group");
                        }
                        let arrow = if collapsed { "⏵" } else { "⏷" };
                        if ui.add(egui::Button::new(RichText::new(arrow).size(11.0).color(Color32::WHITE)).frame(false)).on_hover_text(if collapsed { "Show details" } else { "Minimise" }).clicked() {
                            toggled = true;
                        }
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(RichText::new(type_label).size(11.5).strong().color(Color32::WHITE));
                            if collapsed {
                                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                                    ui.add(egui::Label::new(RichText::new(title).size(12.0).color(Color32::WHITE)).truncate());
                                });
                            }
                        });
                    });
                });
            if !collapsed {
                egui::Frame::new().inner_margin(egui::Margin { left: 9, right: 7, top: 2, bottom: 7 }).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    body(ui);
                });
            }
        })
        .response;
    theme::remember_card(ui, id, outer.rect);
    let clicked = behind.map(|b| {
        if b.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        b.clicked()
    }).unwrap_or(false);
    ui.add_space(4.0);
    (clicked, toggled)
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
        // Text in the cards must not catch clicks: the whole card selects its object.
        ui.style_mut().interaction.selectable_labels = false;

        // ── Header and the one Load button
        ui.horizontal(|ui| {
            ui.label(RichText::new("Scene").size(17.0).strong().color(Color32::WHITE));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.menu_button("⋯", |ui| {
                    if ui.button("New group").clicked() {
                        ops.push(Op::NewGroup(None));
                        ui.close();
                    }
                    if ui.button("Show Camera & Light").on_hover_text("Key the camera and the main light by hand").clicked() {
                        self.show_camera_card = true;
                        self.select_camera();
                        ui.close();
                    }
                    ui.separator();
                    for (k, l) in [("model", "Load model (.pmx / .pmd)…"), ("accessory", "Load prop / stage (.x)…"), ("animation", "Load motion or pose…"), ("audio", "Load song (.wav)…")] {
                        if ui.button(l).clicked() {
                            self.load_dialog(k);
                            ui.close();
                        }
                    }
                });
            });
        });
        let load = egui::Button::new(RichText::new("LOAD…").size(16.0).strong().color(Color32::WHITE)).fill(theme::BG3).min_size(vec2(ui.available_width(), 38.0));
        if ui.add(load).on_hover_text("Models, stages, props, dances, poses, camera motions and songs (several at once). A dance or pose goes onto the selected model, or onto the only model there is.").clicked() {
            self.open_dialog();
        }
        ui.add_space(6.0);
        let empty = self.scene.models.is_empty() && self.scene.groups.is_empty() && self.scene.audio.is_none() && !self.camera_card_visible();
        if empty {
            ui.label(RichText::new("Nothing here yet. Load a model, then a dance and a song, or drop files onto the window.").size(11.0).color(theme::TEXT_DIM));
        }

        // ── Pinned: camera & light, then the song
        if self.camera_card_visible() {
            self.camera_card(ui);
        }
        if self.scene.audio.is_some() {
            self.audio_card(ui);
        }

        // ── Objects and groups
        let choices = self.group_choices();
        self.object_tree(ui, None, &choices, &mut ops);
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
        for op in ops {
            self.apply_op(op);
        }
    }

    pub fn camera_card_visible(&self) -> bool {
        self.show_camera_card || !self.scene.camera_motion.camera.is_empty() || !self.scene.camera_motion.light.is_empty() || self.sel_camera
    }

    pub fn select_camera(&mut self) {
        self.sel_camera = true;
        self.sel_audio = false;
    }

    pub fn select_audio(&mut self) {
        self.sel_audio = true;
        self.sel_camera = false;
    }

    fn card_collapsed(&self, key: &str) -> bool {
        self.card_collapsed.contains(key)
    }

    fn camera_card(&mut self, ui: &mut egui::Ui) {
        let key = "camera".to_string();
        let collapsed = self.card_collapsed(&key);
        let (cam, light) = (self.scene.camera_motion.camera.len(), self.scene.camera_motion.light.len());
        let last = self.scene.camera_motion.camera.last().map(|k| k.frame).unwrap_or(0);
        let mut load = false;
        let (clicked, toggled) = card_shell(ui, ui.make_persistent_id("camera_card"), CAM_COLOR, "Camera & Light", "Camera & Light", self.sel_camera, collapsed, None, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("🎥").size(14.0));
                ui.label(RichText::new("Camera & Light").strong().color(Color32::WHITE));
            });
            ui.label(RichText::new(if cam + light == 0 { "No camera keys yet: select this and press Enter to key the camera.".to_string() } else { format!("{cam} camera keys (to frame {last}) · {light} light keys") }).size(10.5).color(theme::TEXT_DIM));
            if ui.small_button("Load camera motion…").clicked() {
                load = true;
            }
        });
        if toggled {
            self.toggle_card(&key);
        }
        if clicked {
            self.select_camera();
        }
        if load {
            self.load_dialog("motion");
        }
    }

    fn audio_card(&mut self, ui: &mut egui::Ui) {
        let Some(a) = &self.scene.audio else { return };
        let name = crate::findfile::display_name(a.path.file_name().unwrap_or_default());
        let ext = a.path.extension().map(|e| e.to_string_lossy().to_uppercase()).unwrap_or_default();
        let (dur, off, muted, vol) = (a.info.duration, a.offset_frames, a.muted, a.volume);
        let peaks = a.info.peaks.clone();
        let fps = self.scene.fps.max(1.0);
        let frame = self.scene.frame;
        let key = "audio".to_string();
        let collapsed = self.card_collapsed(&key);
        let mut act: Option<&str> = None;
        let (clicked, toggled) = card_shell(ui, ui.make_persistent_id("audio_card"), AUDIO_COLOR, &format!("Audio · {ext}"), &name, self.sel_audio, collapsed, None, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("♪").size(14.0).color(AUDIO_COLOR));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.menu_button("⋯", |ui| {
                        if ui.button("Replace with another song…").clicked() {
                            act = Some("replace");
                            ui.close();
                        }
                        if ui.button(RichText::new("Remove…").color(Color32::from_rgb(255, 130, 120))).clicked() {
                            act = Some("remove");
                            ui.close();
                        }
                    });
                    let icon = if muted || vol <= 0.0 { "🔇" } else { "🔊" };
                    if ui.add(egui::Button::new(icon).frame(false)).on_hover_text(if muted { "Unmute" } else { "Mute" }).clicked() {
                        act = Some("mute");
                    }
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        ui.add(egui::Label::new(RichText::new(&name).strong().color(Color32::WHITE)).truncate());
                    });
                });
            });
            // A small picture of the song, with where the playhead is.
            let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 26.0), Sense::hover());
            let p = ui.painter_at(r);
            p.rect_filled(r, 3.0, theme::BG0);
            if !peaks.is_empty() {
                let n = r.width().max(1.0) as usize;
                for k in 0..n {
                    let idx = k * peaks.len() / n;
                    let v = peaks[idx.min(peaks.len() - 1)] * r.height() * 0.45;
                    let x = r.left() + k as f32;
                    p.line_segment([egui::pos2(x, r.center().y - v), egui::pos2(x, r.center().y + v)], Stroke::new(1.0, AUDIO_COLOR.linear_multiply(0.7)));
                }
                let t = ((frame - off as f32) / fps / dur.max(0.01)).clamp(0.0, 1.0);
                let x = r.left() + t * r.width();
                p.line_segment([egui::pos2(x, r.top()), egui::pos2(x, r.bottom())], Stroke::new(2.0, Color32::WHITE));
            }
            ui.label(RichText::new(format!("{dur:.1} s · starts at frame {off} · volume {:.0}%{}", vol * 100.0, if muted { " (muted)" } else { "" })).size(10.5).color(theme::TEXT_DIM));
        });
        if toggled {
            self.toggle_card(&key);
        }
        if clicked {
            self.select_audio();
        }
        match act {
            Some("replace") => self.load_dialog("audio"),
            Some("remove") => self.ask("Remove the song?", &format!("“{name}” will be taken out of the project (the file stays on disk)."), "Remove", ConfirmAction::RemoveAudio),
            Some("mute") => {
                if let Some(t) = self.scene.audio.as_mut() {
                    t.muted = !t.muted;
                }
            }
            _ => {}
        }
    }

    fn toggle_card(&mut self, key: &str) {
        if !self.card_collapsed.remove(key) {
            self.card_collapsed.insert(key.to_string());
        }
    }

    /// Audio properties (shown in the lower part of the Scene panel when the song is selected).
    pub fn audio_props(&mut self, ui: &mut egui::Ui) {
        theme::section(ui, "Audio properties");
        let fps = self.scene.fps.max(1.0);
        let playhead = self.scene.frame.round() as i32;
        let Some(t) = self.scene.audio.as_mut() else {
            ui.label(RichText::new("No song loaded.").color(theme::TEXT_DIM));
            return;
        };
        let name = crate::findfile::display_name(t.path.file_name().unwrap_or_default());
        theme::card(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.add(egui::Label::new(RichText::new(&name).strong()).truncate()).on_hover_text(t.path.display().to_string());
            ui.label(RichText::new(format!("{:.1} s · {} Hz · {} channel(s) · plays frames {} – {}", t.info.duration, t.info.sample_rate, t.info.channels, t.offset_frames, t.offset_frames + (t.info.duration * fps).round() as i32)).size(10.5).color(theme::TEXT_DIM));
            ui.add_space(4.0);
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
            ui.horizontal(|ui| {
                if theme::reset_label(ui, RichText::new("Starts at frame").size(11.5)) {
                    t.offset_frames = 0;
                }
                ui.add(egui::DragValue::new(&mut t.offset_frames).range(-1_000_000..=1_000_000).speed(0.25))
                    .on_hover_text("Move the song later (higher) or earlier (lower) against the animation. Negative skips the song's beginning.");
                ui.label(RichText::new(format!("{:+.2} s", t.offset_frames as f32 / fps)).size(10.5).color(theme::TEXT_DIM));
            });
            ui.horizontal_wrapped(|ui| {
                for (label, d, tip) in [("−10", -10, "10 frames earlier"), ("−1", -1, "1 frame earlier"), ("+1", 1, "1 frame later"), ("+10", 10, "10 frames later")] {
                    if ui.small_button(label).on_hover_text(tip).clicked() {
                        t.offset_frames += d;
                    }
                }
                if ui.small_button("At playhead").on_hover_text("Start the song at the current frame").clicked() {
                    t.offset_frames = playhead;
                }
            });
        });
        ui.checkbox(&mut self.scene.audio_scrub, RichText::new("Play WAV with frame").size(11.5))
            .on_hover_text("While stopped, each frame you move to plays that frame's bit of the song (for lip-sync and beat timing).");
        ui.horizontal(|ui| {
            if ui.button("Replace song…").clicked() {
                self.load_dialog("audio");
            }
            if ui.button(RichText::new("Remove…").color(Color32::from_rgb(255, 130, 120))).clicked() {
                self.ask("Remove the song?", &format!("“{name}” will be taken out of the project (the file stays on disk)."), "Remove", ConfirmAction::RemoveAudio);
            }
        });
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
        let sel = self.scene.selected == Some(i) && !self.sel_camera && !self.sel_audio;
        let (id, is_acc, vis, shown, name, about, anim, character, group, ext, is_pose_able) = {
            let m = &self.scene.models[i];
            let character = !m.data.rigid_bodies.is_empty() || m.data.bone_index("上半身").is_some() || m.data.bone_index("頭").is_some();
            let (lo, hi) = m.data.bounds();
            let small = (hi - lo).max_element() < 25.0;
            let is_acc = m.is_accessory() || (!character && small);
            let comment = m.data.comment.lines().map(str::trim).find(|l| !l.is_empty()).map(str::to_string);
            let counts = if character {
                format!("{} bones · {} morphs{}", m.data.bones.len(), m.data.morphs.len(), if m.data.rigid_bodies.is_empty() { String::new() } else { format!(" · {} physics parts", m.data.rigid_bodies.len()) })
            } else {
                format!("{} vertices · {} materials", m.data.vertices.len(), m.data.materials.len())
            };
            let about = match comment {
                Some(c) => format!("{c}\n{counts}"),
                None => counts,
            };
            let keys = m.motion.key_count();
            let anim = if keys == 0 {
                "No animation".to_string()
            } else {
                let files: Vec<String> = m.motion_files.iter().map(|p| crate::findfile::display_name(p.file_name().unwrap_or_default())).collect();
                let files = if files.is_empty() { String::new() } else { format!(" · {}", files.join(", ")) };
                format!("{keys} keys · to frame {}{files}", m.motion.max_frame())
            };
            let ext = m.data.path.as_ref().and_then(|p| p.extension()).map(|e| e.to_string_lossy().to_uppercase()).unwrap_or_else(|| "BUILT-IN".into());
            (m.id, is_acc, m.visible, m.shown(), m.name().to_string(), about, anim, character, m.group, ext, !m.is_accessory())
        };
        let (kind, color) = if is_acc {
            ("Prop", PROP_COLOR)
        } else if character {
            ("Model", MODEL_COLOR)
        } else {
            ("Stage", STAGE_COLOR)
        };
        let key = format!("obj:{id}");
        let collapsed = self.card_collapsed(&key);
        let renaming = matches!(&self.renaming, Some((rid, _)) if *rid == id);
        let mut rename_buf = self.renaming.as_ref().filter(|r| r.0 == id).map(|r| r.1.clone()).unwrap_or_default();
        let mut load_anim = false;
        let (clicked, toggled) = card_shell(ui, ui.make_persistent_id(("model_card", id)), color, &format!("{kind} · {ext}"), &name, sel, collapsed, Some((ui.make_persistent_id(("drag_obj", id)), Drag::Object(id))), |ui| {
            ui.horizontal(|ui| {
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
                        if is_pose_able && ui.button("Reset pose").clicked() {
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
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        if renaming {
                            let r = ui.add(egui::TextEdit::singleline(&mut rename_buf).desired_width(ui.available_width()));
                            r.request_focus();
                            if r.lost_focus() || ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                                ops.push(Op::RenameDone);
                            }
                        } else {
                            ui.add(egui::Label::new(RichText::new(&name).strong().size(13.5).color(if shown { Color32::WHITE } else { theme::TEXT_DIM })).truncate());
                        }
                    });
                });
            });
            for line in about.lines() {
                ui.add(egui::Label::new(RichText::new(line).size(10.5).color(theme::TEXT_DIM)).truncate());
            }
            if vis && !shown {
                ui.label(RichText::new("Hidden by its group").size(10.5).color(theme::KEY));
            }
            if is_pose_able {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Animation").size(11.0).strong());
                    ui.add(egui::Label::new(RichText::new(&anim).size(10.5).color(theme::TEXT_DIM)).truncate());
                });
                if ui.small_button("Load animation file…").on_hover_text("A dance (.vmd / .hmo) or pose (.vpd) for this object").clicked() {
                    load_anim = true;
                }
            }
        });
        if renaming {
            if let Some(r) = self.renaming.as_mut() {
                r.1 = rename_buf;
            }
        }
        if toggled {
            self.toggle_card(&key);
        }
        if clicked {
            ops.push(Op::Select(i));
        }
        if load_anim {
            ops.push(Op::Select(i));
            ops.push(Op::LoadAnimation);
        }
    }

    fn apply_op(&mut self, op: Op) {
        match op {
            Op::LoadAnimation => self.load_dialog("animation"),
            Op::Select(i) => {
                if i < self.scene.models.len() {
                    self.scene.selected = Some(i);
                    self.sel_camera = false;
                    self.sel_audio = false;
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
