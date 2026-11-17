//! Welcome screen and project library, confirmations before actions that can't be undone,
//! and unsaved-changes tracking.

use crate::app::{App, Workspace};
use crate::effects::EffectGraph;
use crate::grade::ColorGrade;
use crate::material::SceneLight;
use crate::scene::Scene;
use egui::{Color32, RichText};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RecentProject {
    pub path: PathBuf,
    pub name: String,
    pub opened_secs: u64,
    pub thumb: Option<PathBuf>,
    /// Pinned projects stay at the top of the library and are never dropped from it.
    #[serde(default)]
    pub pinned: bool,
}

pub fn config_dir() -> PathBuf {
    crate::material::presets_path().parent().map(|p| p.to_path_buf()).unwrap_or_else(|| PathBuf::from("."))
}

pub fn load_recent() -> Vec<RecentProject> {
    std::fs::read_to_string(config_dir().join("recent.json")).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

pub fn save_recent(v: &[RecentProject]) {
    let d = config_dir();
    let _ = std::fs::create_dir_all(&d);
    if let Ok(s) = serde_json::to_string_pretty(v) {
        let _ = std::fs::write(d.join("recent.json"), s);
    }
}

pub fn thumb_path_for(project: &Path) -> PathBuf {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    project.hash(&mut h);
    config_dir().join("thumbs").join(format!("{:016x}.png", h.finish()))
}

fn now_secs() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// "2026-11-10 14:05" (UTC) from Unix seconds, without a date library.
pub fn fmt_date(secs: u64) -> String {
    let days = (secs / 86400) as i64;
    let rem = secs % 86400;
    // civil-from-days (proleptic Gregorian)
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02} {:02}:{:02} UTC", rem / 3600, (rem / 60) % 60)
}

/// Actions that need a "are you sure?" first.
#[derive(Clone)]
pub enum ConfirmAction {
    DeleteModel(u64),
    DeleteGroup(u64),
    NewProject,
    OpenProject(PathBuf),
    OpenProjectDialog,
    OpenSample,
    GoHome,
    Quit,
    ClearScreenFx,
    ClearGraph,
    ReplaceGraph(String),
    MaterialAuto(String),
    RemoveAudio,
    ClearMotion(u64),
    ClearCameraMotion,
    SetLights(Vec<SceneLight>, String),
}

#[derive(Clone)]
pub struct Confirm {
    pub title: String,
    pub message: String,
    pub ok: String,
    pub action: ConfirmAction,
}

impl App {
    pub fn ask(&mut self, title: &str, message: &str, ok: &str, action: ConfirmAction) {
        self.confirm = Some(Confirm { title: title.into(), message: message.into(), ok: ok.into(), action });
    }

    /// Ask only when there are unsaved changes; otherwise do it straight away.
    pub fn guard_unsaved(&mut self, action: ConfirmAction, what: &str) {
        self.refresh_dirty(true);
        if self.dirty {
            self.ask("Unsaved changes", &format!("Your project has changes that aren't saved. {what} anyway? The changes will be lost."), "Discard changes", action);
        } else {
            self.run_confirmed(action);
        }
    }

    pub fn run_confirmed(&mut self, action: ConfirmAction) {
        match action {
            ConfirmAction::DeleteModel(id) => {
                if let Some(i) = self.scene.model_by_id(id) {
                    let name = self.scene.models[i].name().to_string();
                    self.scene.remove_model(i);
                    self.sel_bone = None;
                    self.mat_sel = None;
                    self.set_status(format!("Removed “{name}”"));
                }
            }
            ConfirmAction::DeleteGroup(id) => {
                let name = self.scene.group_index(id).map(|g| self.scene.groups[g].name.clone()).unwrap_or_default();
                self.scene.delete_group(id, false);
                self.sel_bone = None;
                self.mat_sel = None;
                self.set_status(format!("Deleted group “{name}” and everything in it"));
            }
            ConfirmAction::NewProject => self.new_project(),
            ConfirmAction::OpenProject(p) => {
                match self.load_project(&p) {
                    Ok(m) => self.set_status(m),
                    Err(e) => self.set_error(e),
                }
                self.home = false;
            }
            ConfirmAction::OpenProjectDialog => {
                if let Some(p) = rfd::FileDialog::new().add_filter("Hanabi project", &["hproj"]).pick_file() {
                    match self.load_project(&p) {
                        Ok(m) => self.set_status(m),
                        Err(e) => self.set_error(e),
                    }
                    self.home = false;
                }
            }
            ConfirmAction::OpenSample => self.open_sample(),
            ConfirmAction::GoHome => {
                self.audio.stop();
                self.scene.playing = false;
                self.recent = load_recent();
                self.home = true;
            }
            ConfirmAction::Quit => {
                self.allow_close = true;
                self.ctx_close = true;
            }
            ConfirmAction::ClearScreenFx => {
                for id in self.graph.chain_ids() {
                    self.graph.remove(id);
                }
            }
            ConfirmAction::ClearGraph => {
                self.graph = EffectGraph::empty();
                self.nodes.multi.clear();
                self.nodes.selected = None;
                self.nodes.fit_pending = true;
            }
            ConfirmAction::ReplaceGraph(name) => {
                self.apply_preset(&name, true);
            }
            ConfirmAction::MaterialAuto(name) => {
                self.graph.mat_fx.remove(&name);
            }
            ConfirmAction::RemoveAudio => {
                self.audio.stop();
                self.scene.audio = None;
            }
            ConfirmAction::ClearMotion(id) => {
                if let Some(i) = self.scene.model_by_id(id) {
                    self.checkpoint();
                    let name = self.scene.models[i].motion.model_name.clone();
                    self.scene.models[i].motion = crate::motion::Motion { model_name: name, ..Default::default() };
                    self.set_status("Cleared the motion (Ctrl+Z to undo)");
                }
            }
            ConfirmAction::ClearCameraMotion => {
                self.checkpoint();
                self.scene.camera_motion = Default::default();
                self.set_status("Cleared the camera motion (Ctrl+Z to undo)");
            }
            ConfirmAction::SetLights(l, name) => {
                self.graph.lights = l;
                self.light_sel = None;
                self.set_status(format!("Loaded light rig “{name}”"));
            }
        }
    }

    pub fn confirm_window(&mut self, ctx: &egui::Context) {
        let Some(c) = self.confirm.clone() else { return };
        let mut result = None;
        egui::Modal::new(egui::Id::new("confirm")).show(ctx, |ui| {
            ui.set_width(380.0);
            ui.label(RichText::new(&c.title).strong().size(16.0));
            ui.add_space(4.0);
            ui.label(&c.message);
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if ui.add(egui::Button::new(RichText::new(&c.ok).color(Color32::WHITE)).fill(Color32::from_rgb(190, 60, 70))).clicked() {
                    result = Some(true);
                }
                if ui.button("Cancel").clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    result = Some(false);
                }
            });
        });
        match result {
            Some(true) => {
                self.confirm = None;
                self.run_confirmed(c.action);
            }
            Some(false) => self.confirm = None,
            None => {}
        }
    }

    /// Compare the project with what was last saved (checked every second, or now when `force`).
    pub fn refresh_dirty(&mut self, force: bool) {
        if !force && self.frame_count % 60 != 0 {
            return;
        }
        // Settings are compared through a small fingerprint (no keyframes); keyframe edits are
        // counted through undo steps. Serialising a dense dance every second made the app stutter.
        let cur = crate::project::fingerprint(&self.scene, &self.graph, &self.grade);
        self.dirty = cur != self.saved_state || self.edit_rev != self.saved_rev;
    }

    pub fn mark_saved(&mut self) {
        self.saved_state = crate::project::fingerprint(&self.scene, &self.graph, &self.grade);
        self.saved_rev = self.edit_rev;
        self.dirty = false;
    }

    pub fn new_project(&mut self) {
        self.audio.stop();
        self.scene = Scene::new();
        self.graph = EffectGraph::empty();
        self.grade = ColorGrade::default();
        self.project_path = None;
        self.undo.clear();
        self.redo.clear();
        self.sel_bone = None;
        self.mat_sel = None;
        self.home = false;
        self.ws = Workspace::Animation;
        self.mark_saved();
        self.set_status("New project. Load a model with ＋ Model, then a motion and a song.");
    }

    /// The built-in sample: the demo mannequin with its dance.
    pub fn open_sample(&mut self) {
        self.new_project();
        let i = self.scene.add_model(crate::demo::mannequin());
        self.scene.models[i].motion = crate::demo::dance();
        self.scene.frame = 42.0;
        self.scene.play_end = 240;
        self.sel_bone = self.scene.models[i].data.bone_index("arm_L");
        self.mark_saved();
        self.set_status("Opened the sample project (demo mannequin and dance).");
    }

    pub fn add_recent(&mut self, path: &Path) {
        let mut v = load_recent();
        let pinned = v.iter().any(|r| r.path == path && r.pinned);
        v.retain(|r| r.path != path);
        let name = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "project".into());
        let thumb = Some(thumb_path_for(path)).filter(|t| t.exists());
        v.insert(0, RecentProject { path: path.to_path_buf(), name, opened_secs: now_secs(), thumb, pinned });
        // Keep up to 100 recent projects (pinned ones always stay).
        let mut unpinned = 0;
        v.retain(|r| {
            if r.pinned {
                return true;
            }
            unpinned += 1;
            unpinned <= 100
        });
        save_recent(&v);
        self.recent = v;
    }

    /// After saving, take a small picture of the scene for the project library.
    pub fn take_pending_thumbnail(&mut self, frame: &mut eframe::Frame) {
        let Some(project) = self.pending_thumb.take() else { return };
        let Some(gl) = frame.gl().cloned() else { return };
        if let Some((w, h, px)) = self.render_offline(&gl, 320, 180, 0.0, true) {
            let t = thumb_path_for(&project);
            if let Some(d) = t.parent() {
                let _ = std::fs::create_dir_all(d);
            }
            if image::save_buffer(&t, &px, w, h, image::ColorType::Rgba8).is_ok() {
                self.thumbs.remove(&t);
                self.add_recent(&project);
            }
        }
    }

    pub fn thumb_texture(&mut self, ctx: &egui::Context, path: &Path) -> Option<egui::TextureHandle> {
        if let Some(t) = self.thumbs.get(path) {
            return t.clone();
        }
        let tex = image::open(path).ok().map(|img| {
            let rgba = img.to_rgba8();
            let size = [rgba.width() as usize, rgba.height() as usize];
            ctx.load_texture(path.display().to_string(), egui::ColorImage::from_rgba_unmultiplied(size, rgba.as_raw()), egui::TextureOptions::LINEAR)
        });
        self.thumbs.insert(path.to_path_buf(), tex.clone());
        tex
    }
}
