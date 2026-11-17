//! Application state, main loop and the shared chrome (top bar, menus, status bar).

use crate::audio::AudioPlayer;
use crate::effects::{presets, EffectGraph};
use crate::export::{RenderJob, RenderSettings};
use crate::grade::ColorGrade;
use crate::mcp::Mcp;
use crate::motion::{Curve, Motion, TrackId};
use crate::render::{FrameParams, Renderer};
use crate::scene::Scene;
use crate::tools::PoseClip;
use crate::ui::theme;
use egui::{Color32, RichText};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Workspace {
    Animation,
    Effects,
    Color,
    Render,
    Graph,
}

impl Workspace {
    pub const ALL: [Workspace; 5] = [Workspace::Animation, Workspace::Effects, Workspace::Color, Workspace::Render, Workspace::Graph];
    pub fn label(self) -> &'static str {
        match self {
            Workspace::Animation => "Animation",
            Workspace::Effects => "Effects",
            Workspace::Color => "Color",
            Workspace::Render => "Render",
            Workspace::Graph => "Graph",
        }
    }
    pub fn icon(self) -> &'static str {
        match self {
            Workspace::Animation => "🎞",
            Workspace::Effects => "✨",
            Workspace::Color => "🎨",
            Workspace::Render => "🎬",
            Workspace::Graph => "🔗",
        }
    }
}

/// How the viewport shows the final render's framing.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FrameMode {
    /// Use the whole viewport freely.
    Full,
    /// Show the whole viewport but mark (and dim outside) the output frame.
    Guide,
    /// Show only the output frame.
    Only,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum GizmoMode {
    Rotate,
    Move,
}

#[derive(Clone, Debug)]
pub struct GizmoDrag {
    pub axis: usize,
    pub last: egui::Pos2,
}

pub struct TimelineState {
    pub px_per_frame: f32,
    pub scroll: f32,
    pub collapsed: std::collections::HashSet<String>,
    pub drag_from: Option<(f32, u32)>,
    pub drag_delta: i32,
    pub box_start: Option<egui::Pos2>,
    /// Keep the playhead in view (on by default).
    pub follow: bool,
    pub last_frame: f32,
    pub bar_drag: Option<(f32, f32)>,
}

impl Default for TimelineState {
    fn default() -> Self {
        Self {
            px_per_frame: 6.0,
            scroll: 0.0,
            collapsed: Default::default(),
            drag_from: None,
            drag_delta: 0,
            box_start: None,
            follow: true,
            last_frame: -1.0,
            bar_drag: None,
        }
    }
}

#[derive(Default)]
pub struct NodeUi {
    pub pan: egui::Vec2,
    pub zoom: f32,
    pub selected: Option<u32>,
    pub dragging_node: Option<u32>,
    pub link_from: Option<u32>,
    pub add_menu_at: Option<egui::Pos2>,
    pub fit_pending: bool,
    pub fit_size: egui::Vec2,
    pub touched: bool,
    /// All selected nodes (box select / shift-click); `selected` is the one shown in the inspector.
    pub multi: Vec<u32>,
    pub box_start: Option<egui::Pos2>,
    pub preset_menu_at: Option<egui::Pos2>,
}

#[derive(Clone)]
pub struct Snapshot {
    pub motions: Vec<(u64, Motion)>,
    pub camera: Motion,
}

pub enum KeyClip {
    Bone(crate::motion::BoneKey),
    Morph(crate::motion::MorphKey),
    Camera(crate::motion::CameraKey),
    Light(crate::motion::LightKey),
}

pub struct ScreenshotState {
    pub path: PathBuf,
    pub requested: bool,
    pub started: Instant,
    pub requested_at: Option<Instant>,
}

pub struct App {
    pub scene: Scene,
    pub graph: EffectGraph,
    pub grade: ColorGrade,
    pub renderer: Option<Renderer>,
    pub render_error: Option<String>,
    pub viewport_tex: Option<(glow::Texture, egui::TextureId)>,
    pub ws: Workspace,
    pub sel_bone: Option<usize>,
    /// Other selected bones (model id, bone indices); the active bone is `sel_bone`.
    pub sel_bones: (u64, std::collections::BTreeSet<usize>),
    /// Inspector tab in the Animation workspace: 0 pose, 1 expressions, 2 tools.
    pub insp_tab: u8,
    pub bone_search: String,
    pub morph_search: String,
    pub tool: crate::ui::animtools::ToolParams,
    pub sel_morph: Option<usize>,
    pub sel_camera: bool,
    pub sel_keys: Vec<(TrackId, u32)>,
    pub key_clip: Vec<(TrackId, i64, KeyClip)>,
    pub pose_clip: Option<PoseClip>,
    pub tl: TimelineState,
    pub gizmo: GizmoMode,
    pub gizmo_local: bool,
    pub gizmo_drag: Option<GizmoDrag>,
    pub gizmo_hover: Option<usize>,
    pub auto_key: bool,
    pub default_curve: Curve,
    pub curve_all_channels: bool,
    pub undo: Vec<Snapshot>,
    pub redo: Vec<Snapshot>,
    pub nodes: NodeUi,
    pub render_settings: RenderSettings,
    pub job: Option<RenderJob>,
    pub mcp: Option<Mcp>,
    pub status: String,
    pub status_err: bool,
    pub screenshot: Option<ScreenshotState>,
    pub last_tick: Instant,
    pub fps: f32,
    pub audio: AudioPlayer,
    pub project_path: Option<PathBuf>,
    pub frame_count: u64,
    pub wall_extra: f32,
    pub histogram: Vec<[f32; 3]>,
    pub hist_tick: u32,
    pub show_help: bool,
    pub show_mcp: bool,
    pub show_video_help: bool,
    /// Playhead jumps the song has caught up with, and when the last one happened.
    audio_seek_rev: u64,
    audio_seek_at: Option<Instant>,
    audio_seen_offset: i32,
    pub last_autosave: std::time::Instant,
    pub autosave_busy: std::sync::Arc<std::sync::atomic::AtomicBool>,
    pub show_credits: bool,
    pub show_physics: bool,
    /// Part selected in the physics panel: (model id, is joint, index).
    pub phys_sel: Option<(u64, bool, usize)>,
    pub phys_tab: u8,
    pub phys_search: String,
    pub phys_list_joints: bool,
    /// A model whose part changes still need a rebuild (done when the mouse is released).
    pub phys_pending: Option<u64>,
    pub quit_at: Option<Instant>,
    pub phys_undo: Vec<crate::ui::phystune_ui::PhysState>,
    pub phys_redo: Vec<crate::ui::phystune_ui::PhysState>,
    /// An undo step is being collected (mouse still down), so a drag counts once.
    pub phys_edit_open: bool,
    /// A physics change that failed the stability check: (model id, message).
    pub phys_warning: Option<(u64, String)>,
    pub phys_blowups_seen: u32,
    pub phys_flips_seen: u32,
    pub phys_win_rect: egui::Rect,
    pub confirm: Option<crate::ui::home::Confirm>,
    pub home: bool,
    pub recent: Vec<crate::ui::home::RecentProject>,
    pub home_ui: crate::ui::welcome::HomeUi,
    pub thumbs: std::collections::HashMap<PathBuf, Option<egui::TextureHandle>>,
    pub saved_state: String,
    pub dirty: bool,
    /// Counts keyframe edits (undo steps) for the unsaved-changes dot.
    pub edit_rev: u64,
    pub saved_rev: u64,
    pub allow_close: bool,
    pub ctx_close: bool,
    pub pending_thumb: Option<PathBuf>,
    pub renaming: Option<(u64, String)>,
    pub gen_kind: crate::tools::Generator,
    pub gen_length: u32,
    pub gen_bpm: f32,
    pub viewport_rect: egui::Rect,
    pub last_render_dt: f32,
    pub new_model_reset: bool,
    pub frame_mode: FrameMode,
    pub view_proj: Option<glam::Mat4>,
    /// Effects workspace: simple stack (true) or node graph (false).
    pub fx_simple: bool,
    pub fx_tab: FxTab,
    pub pg: crate::ui::projgraph::ProjGraphUi,
    /// Material being edited: (model index, material index).
    pub mat_sel: Option<(usize, usize)>,
    pub mat_nodes_open: bool,
    pub mnodes: MatNodeUi,
    pub user_presets: crate::material::UserPresets,
    pub light_sel: Option<usize>,
    pub light_drag: Option<usize>,
    pub preset_name_buf: String,
}

/// Sections of the Effects workspace.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FxTab {
    Lights,
    Materials,
    Stage,
    Screen,
}

/// View state of the material node editor.
#[derive(Default)]
pub struct MatNodeUi {
    pub pan: egui::Vec2,
    pub zoom: f32,
    pub selected: Option<u32>,
    pub link_from: Option<u32>,
    pub dragging: Option<u32>,
    pub add_menu_at: Option<egui::Pos2>,
}

pub struct Args {
    pub screenshot: Option<PathBuf>,
    pub mcp_stdio: bool,
    pub mcp_port: u16,
    pub workspace: Option<String>,
    pub files: Vec<PathBuf>,
    pub preset: Option<String>,
    pub no_demo: bool,
    pub sample: bool,
    pub home: bool,
    /// `--frame N`: play the timeline (with physics) up to frame N before showing it.
    pub frame: Option<u32>,
    /// `--show credits|video-help|physics|mcp`: open a window at start (for pictures and docs).
    pub show: Option<String>,
    /// `--quit-after S`: close by itself after S seconds (for automated tests, e.g. of MCP).
    pub quit_after: Option<f32>,
}

impl App {
    pub fn new(cc: &eframe::CreationContext, args: Args) -> Self {
        theme::apply(&cc.egui_ctx);
        let (renderer, render_error) = match cc.gl.as_ref() {
            Some(gl) => match Renderer::new(gl) {
                Ok(r) => (Some(r), None),
                Err(e) => (None, Some(e)),
            },
            None => (None, Some("No OpenGL context".into())),
        };
        let mut scene = Scene::new();
        // The sample (demo mannequin + dance) opens directly for screenshots and `--sample`;
        // otherwise the app starts on the welcome screen unless files were given.
        let start_home = (args.screenshot.is_none() && !args.sample && args.files.is_empty()) || args.home;
        if !args.no_demo && !start_home {
            let idx = scene.add_model(crate::demo::mannequin());
            scene.models[idx].motion = crate::demo::dance();
            scene.frame = 42.0;
            scene.play_end = 240;
        }
        // The node graph starts with only Scene Render → Output; presets are added from the drawer.
        let preset_arg = args.preset.clone().unwrap_or_else(|| "No Effects".into());
        let mut preset_list = preset_arg.split('+').map(|s| s.trim().to_string());
        let preset_name = preset_list.next().unwrap_or_default();
        let extra_presets: Vec<String> = preset_list.collect();
        let graph = presets()
            .into_iter()
            .find(|p| p.name.eq_ignore_ascii_case(&preset_name))
            .map(|p| (p.build)())
            .unwrap_or_else(EffectGraph::empty);
        let ws = match args.workspace.as_deref().map(|s| s.to_ascii_lowercase()) {
            Some(s) if s.starts_with("eff") => Workspace::Effects,
            Some(s) if s.starts_with("gra") => Workspace::Graph,
            Some(s) if s.starts_with("col") => Workspace::Color,
            Some(s) if s.starts_with("ren") => Workspace::Render,
            _ => Workspace::Animation,
        };
        let mcp = Some(Mcp::start(args.mcp_port, args.mcp_stdio, cc.egui_ctx.clone()));
        let mut app = Self {
            scene,
            graph,
            grade: ColorGrade::default(),
            renderer,
            render_error,
            viewport_tex: None,
            ws,
            sel_bone: None,
            sel_bones: (0, Default::default()),
            insp_tab: 0,
            bone_search: String::new(),
            morph_search: String::new(),
            tool: Default::default(),
            sel_morph: None,
            sel_camera: false,
            sel_keys: Vec::new(),
            key_clip: Vec::new(),
            pose_clip: None,
            tl: TimelineState::default(),
            gizmo: GizmoMode::Rotate,
            gizmo_local: false,
            gizmo_drag: None,
            gizmo_hover: None,
            auto_key: false,
            default_curve: Curve::EASE_IN_OUT,
            curve_all_channels: true,
            undo: Vec::new(),
            redo: Vec::new(),
            nodes: NodeUi { zoom: 1.0, pan: egui::vec2(260.0, 40.0), fit_pending: true, ..Default::default() },
            render_settings: RenderSettings::default(),
            job: None,
            mcp,
            status: "Welcome! Drop a .pmx/.pmd model, .vmd motion or .wav song onto the window to begin.".into(),
            status_err: false,
            screenshot: args.screenshot.map(|path| ScreenshotState { path, requested: false, started: Instant::now(), requested_at: None }),
            last_tick: Instant::now(),
            fps: 60.0,
            audio: AudioPlayer::new(),
            project_path: None,
            frame_count: 0,
            wall_extra: 0.0,
            histogram: Vec::new(),
            hist_tick: 0,
            show_help: false,
            show_mcp: false,
            show_video_help: false,
            audio_seek_rev: 0,
            audio_seek_at: None,
            audio_seen_offset: 0,
            last_autosave: Instant::now(),
            autosave_busy: Default::default(),
            show_credits: false,
            show_physics: false,
            phys_sel: None,
            phys_tab: 0,
            phys_search: String::new(),
            phys_list_joints: false,
            phys_pending: None,
            quit_at: args.quit_after.map(|s| Instant::now() + std::time::Duration::from_secs_f32(s)),
            phys_undo: Vec::new(),
            phys_redo: Vec::new(),
            phys_edit_open: false,
            phys_warning: None,
            phys_blowups_seen: 0,
            phys_flips_seen: 0,
            phys_win_rect: egui::Rect::NOTHING,
            confirm: None,
            home: start_home,
            recent: crate::ui::home::load_recent(),
            home_ui: Default::default(),
            thumbs: Default::default(),
            saved_state: String::new(),
            dirty: false,
            edit_rev: 0,
            saved_rev: 0,
            allow_close: false,
            ctx_close: false,
            pending_thumb: None,
            renaming: None,
            gen_kind: crate::tools::Generator::Idle,
            gen_length: 120,
            gen_bpm: 120.0,
            viewport_rect: egui::Rect::NOTHING,
            last_render_dt: 0.0,
            new_model_reset: false,
            frame_mode: FrameMode::Guide,
            view_proj: None,
            fx_simple: true,
            pg: Default::default(),
            fx_tab: FxTab::Lights,
            mat_sel: None,
            mat_nodes_open: false,
            mnodes: MatNodeUi { zoom: 1.0, pan: egui::vec2(40.0, 30.0), ..Default::default() },
            user_presets: crate::material::UserPresets::load(),
            light_sel: None,
            light_drag: None,
            preset_name_buf: String::new(),
        };
        for p in extra_presets {
            app.apply_preset(&p, false);
        }
        app.mark_saved();
        // "--workspace effects:materials" opens a section of the Effects workspace.
        if let Some(tab) = args.workspace.as_deref().and_then(|w| w.split_once(':')).map(|(_, t)| t.to_ascii_lowercase()) {
            app.fx_tab = match tab.as_str() {
                t if t.starts_with("mat") => FxTab::Materials,
                t if t.starts_with("sta") => FxTab::Stage,
                t if t.starts_with("cam") || t.starts_with("scr") => FxTab::Screen,
                _ => FxTab::Lights,
            };
            if app.fx_tab == FxTab::Materials && !app.scene.models.is_empty() {
                app.mat_sel = Some((0, 3));
            }
        }
        if let Some(s) = app.scene.models.first() {
            app.sel_bone = s.data.bone_index("arm_L");
        }
        for f in args.files {
            app.open_path(&f);
        }
        match args.show.as_deref() {
            Some("credits") => app.show_credits = true,
            Some("video-help") => app.show_video_help = true,
            Some("physics") => app.show_physics = true,
            Some("pose") | Some("tools") | Some("expressions") => {
                // Animation inspector tabs with a few bones picked (for pictures).
                app.insp_tab = match args.show.as_deref() {
                    Some("tools") => 2,
                    Some("expressions") => 1,
                    _ => 0,
                };
                if let Some(mi) = app.anim_model() {
                    if let Some(b) = app.scene.models[mi].data.bone_index("左腕") {
                        app.sel_bone = Some(b);
                        app.select_op(crate::ui::animtools::SelOp::Mirror);
                        app.sel_bone = Some(b);
                    }
                }
            }
            Some("camera") => app.sel_camera = true,
            Some("physics-parts") => {
                // Parts tab with the selected model's first simulated body picked and the
                // wireframes on (for pictures of the tuning panel).
                app.show_physics = true;
                app.phys_tab = 1;
                app.scene.stage.show_rigid = true;
                app.scene.stage.show_joints = true;
                if let Some(m) = app.scene.selected.and_then(|i| app.scene.models.get(i)) {
                    let i = m.data.rigid_bodies.iter().position(|b| b.mode != crate::model::RigidMode::FollowBone).unwrap_or(0);
                    app.phys_sel = Some((m.id, false, i));
                }
            }
            Some("mcp") => app.show_mcp = true,
            _ => {}
        }
        if let Some(n) = args.frame {
            app.scene.playing = true;
            app.scene.force_reset_physics();
            for f in 0..=n {
                app.scene.frame = f as f32;
                app.scene.evaluate();
            }
            app.scene.playing = false;
        }
        app
    }

    pub fn set_status(&mut self, s: impl Into<String>) {
        self.status = s.into();
        self.status_err = false;
    }
    pub fn set_error(&mut self, s: impl Into<String>) {
        self.status = s.into();
        self.status_err = true;
    }

    pub fn checkpoint(&mut self) {
        self.edit_rev += 1;
        let snap = Snapshot {
            motions: self.scene.models.iter().map(|m| (m.id, m.motion.clone())).collect(),
            camera: self.scene.camera_motion.clone(),
        };
        self.undo.push(snap);
        // Each step copies every key; with dense captured motions (thousands of keys per bone)
        // 80 steps would take gigabytes, so keep as many as fit in about two million keys.
        let keys = |s: &Snapshot| s.motions.iter().map(|(_, m)| m.key_count()).sum::<usize>() + s.camera.key_count();
        while self.undo.len() > 2 && (self.undo.len() > 80 || self.undo.iter().map(keys).sum::<usize>() > 2_000_000) {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    fn restore(&mut self, s: Snapshot) {
        for (id, m) in s.motions {
            if let Some(i) = self.scene.model_by_id(id) {
                self.scene.models[i].motion = m;
            }
        }
        self.scene.camera_motion = s.camera;
    }

    fn current_snapshot(&self) -> Snapshot {
        Snapshot { motions: self.scene.models.iter().map(|m| (m.id, m.motion.clone())).collect(), camera: self.scene.camera_motion.clone() }
    }

    pub fn do_undo(&mut self) {
        self.edit_rev += 1;
        if let Some(s) = self.undo.pop() {
            let cur = self.current_snapshot();
            self.redo.push(cur);
            self.restore(s);
            self.set_status("Undo");
        }
    }
    pub fn do_redo(&mut self) {
        self.edit_rev += 1;
        if let Some(s) = self.redo.pop() {
            let cur = self.current_snapshot();
            self.undo.push(cur);
            self.restore(s);
            self.set_status("Redo");
        }
    }

    pub fn open_path(&mut self, p: &std::path::Path) {
        let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
        if ext == "hproj" {
            self.guard_unsaved(crate::ui::home::ConfirmAction::OpenProject(p.to_path_buf()), "Open another project");
            return;
        }
        let is_motion = matches!(ext.as_str(), "vmd" | "hmo");
        if is_motion {
            self.checkpoint();
        }
        match self.scene.load_path(p) {
            Ok(msg) => {
                if matches!(ext.as_str(), "pmx" | "pmd" | "x") {
                    self.sel_bone = None;
                    self.sel_morph = None;
                    // Attach new accessories to the previously selected model's centre? Keep at origin.
                }
                self.set_status(msg)
            }
            Err(e) => self.set_error(e),
        }
    }

    /// File picker for one kind of file: "model", "accessory", "motion", "pose", "audio" or "project".
    pub fn load_dialog(&mut self, kind: &str) {
        let (title, name, exts): (&str, &str, &[&str]) = match kind {
            "model" => ("Load model", "Models (PMX / PMD)", &["pmx", "pmd"]),
            "accessory" => ("Load accessory / stage", "Accessories (X)", &["x"]),
            "motion" => ("Load motion", "Motions (VMD / HMO)", &["vmd", "hmo"]),
            "pose" => ("Load pose", "Poses (VPD)", &["vpd"]),
            "audio" => ("Load audio", "Audio (WAV)", &["wav"]),
            _ => ("Open project", "Hanabi project", &["hproj"]),
        };
        let dlg = rfd::FileDialog::new().set_title(title).add_filter(name, exts);
        let files = if kind == "project" { dlg.pick_file().map(|f| vec![f]) } else { dlg.pick_files() };
        if let Some(files) = files {
            for f in files {
                self.open_path(&f);
            }
        }
    }

    pub fn open_dialog(&mut self) {
        if let Some(files) = rfd::FileDialog::new()
            .add_filter("All supported", &["pmx", "pmd", "x", "vmd", "hmo", "vpd", "wav", "hproj"])
            .add_filter("Models", &["pmx", "pmd"])
            .add_filter("Accessories", &["x"])
            .add_filter("Motions", &["vmd", "hmo"])
            .add_filter("Poses", &["vpd"])
            .add_filter("Audio", &["wav"])
            .add_filter("Projects", &["hproj"])
            .pick_files()
        {
            for f in files {
                self.open_path(&f);
            }
        }
    }

    pub fn save_motion_to(&mut self, path: &std::path::Path, camera: bool) -> Result<String, String> {
        let motion = if camera {
            self.scene.camera_motion.clone()
        } else {
            let i = self.scene.selected.ok_or("Select a model first")?;
            let mut m = self.scene.models[i].motion.clone();
            m.model_name = self.scene.models[i].data.name.clone();
            m
        };
        let count = motion.key_count();
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
        if ext == "hmo" {
            let s = crate::motion::save_own(&motion, self.scene.fps)?;
            std::fs::write(path, s).map_err(|e| e.to_string())?;
        } else {
            let mut motion = motion;
            if camera {
                motion.model_name = "カメラ・照明".into();
            }
            std::fs::write(path, crate::formats::vmd::write(&motion)).map_err(|e| e.to_string())?;
        }
        Ok(format!("Saved {} keys to {}", count, path.display()))
    }

    pub fn save_motion_dialog(&mut self, camera: bool) {
        let name = if camera { "camera".to_string() } else { self.scene.selected.map(|i| self.scene.models[i].data.name.clone()).unwrap_or_else(|| "motion".into()) };
        if let Some(p) = rfd::FileDialog::new()
            .set_file_name(format!("{name}.vmd"))
            .add_filter("MMD-compatible motion", &["vmd"])
            .add_filter("Hanabi motion (readable)", &["hmo"])
            .save_file()
        {
            match self.save_motion_to(&p, camera) {
                Ok(m) => self.set_status(m),
                Err(e) => self.set_error(e),
            }
        }
    }

    pub fn save_pose_dialog(&mut self) {
        let Some(i) = self.scene.selected else { return };
        let m = &self.scene.models[i];
        let mut pose = crate::formats::vpd::Pose { model_name: m.data.name.clone(), ..Default::default() };
        for (bi, b) in m.data.bones.iter().enumerate() {
            let (p, r) = m.bone_pose(bi);
            if p.length_squared() > 1e-8 || r.angle_between(glam::Quat::IDENTITY) > 1e-4 {
                pose.bones.push((b.name.clone(), p, r));
            }
        }
        if let Some(path) = rfd::FileDialog::new().set_file_name("pose.vpd").add_filter("Pose", &["vpd"]).save_file() {
            match std::fs::write(&path, crate::formats::vpd::write(&pose)) {
                Ok(_) => self.set_status(format!("Saved pose ({} bones)", pose.bones.len())),
                Err(e) => self.set_error(e.to_string()),
            }
        }
    }

    pub fn save_project_to(&mut self, path: &std::path::Path) -> Result<String, String> {
        let s = crate::project::save(&self.scene, &self.graph, &self.grade)?;
        crate::project::write_safely(path, &s, true)?;
        // The project is saved, so its automatic backup is no longer needed.
        let _ = std::fs::remove_file(crate::project::autosave_path(Some(path)));
        self.project_path = Some(path.to_path_buf());
        self.mark_saved();
        self.pending_thumb = Some(path.to_path_buf());
        self.add_recent(path);
        Ok(format!("Project saved to {}", path.display()))
    }

    /// Every three minutes, back up a project with unsaved changes (written in the background).
    fn autosave_tick(&mut self) {
        if self.screenshot.is_some() || self.home || !self.dirty {
            return;
        }
        if self.last_autosave.elapsed().as_secs() < 180 || self.autosave_busy.load(std::sync::atomic::Ordering::SeqCst) {
            return;
        }
        self.last_autosave = std::time::Instant::now();
        let Ok(text) = crate::project::save(&self.scene, &self.graph, &self.grade) else { return };
        let path = crate::project::autosave_path(self.project_path.as_deref());
        let busy = self.autosave_busy.clone();
        busy.store(true, std::sync::atomic::Ordering::SeqCst);
        std::thread::spawn(move || {
            let _ = crate::project::write_safely(&path, &text, false);
            busy.store(false, std::sync::atomic::Ordering::SeqCst);
        });
    }

    fn open_autosave(&mut self, auto: std::path::PathBuf) {
        let keep = self.project_path.clone();
        match self.load_project(&auto) {
            Ok(_) => {
                // Keep working on the real project file; the next save replaces it.
                self.project_path = keep;
                self.edit_rev += 1;
                self.set_status("Opened the last autosave. Save to keep it.");
            }
            Err(e) => self.set_error(e),
        }
    }

    pub fn save_project_dialog(&mut self, save_as: bool) {
        let path = match (&self.project_path, save_as) {
            (Some(p), false) => Some(p.clone()),
            _ => rfd::FileDialog::new().set_file_name("project.hproj").add_filter("Hanabi project", &["hproj"]).save_file(),
        };
        if let Some(p) = path {
            match self.save_project_to(&p) {
                Ok(m) => self.set_status(m),
                Err(e) => self.set_error(e),
            }
        }
    }

    pub fn load_project(&mut self, path: &std::path::Path) -> Result<String, String> {
        let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
        let p = crate::project::load(&text)?;
        let mut scene = Scene::new();
        let mut problems = Vec::new();
        let mut placed: Vec<(usize, Option<usize>)> = Vec::new();
        for pm in &p.models {
            let idx = if pm.demo {
                Some(scene.add_model(crate::demo::mannequin()))
            } else if let Some(path) = &pm.path {
                match scene.load_path(path) {
                    Ok(_) => Some(scene.models.len() - 1),
                    Err(e) => {
                        problems.push(e);
                        None
                    }
                }
            } else {
                None
            };
            if let Some(i) = idx {
                placed.push((i, pm.group));
                let m = &mut scene.models[i];
                m.motion = pm.motion.clone();
                m.visible = pm.visible;
                m.physics_enabled = pm.physics;
                m.edge_width = pm.edge_width;
                m.acc_pos = pm.acc_pos;
                m.acc_rot = pm.acc_rot;
                m.acc_scale = pm.acc_scale;
                m.acc_opacity = pm.acc_opacity;
            }
        }
        // attachments after all models exist
        for (i, pm) in p.models.iter().enumerate() {
            if let (Some((pi, bone)), true) = (&pm.attach, i < scene.models.len()) {
                if let Some(parent) = scene.models.get(*pi) {
                    let pid = parent.id;
                    if let Some(bi) = parent.data.bone_index(bone) {
                        scene.models[i].attach = Some((pid, bi));
                    }
                }
            }
        }
        scene.camera_motion = p.camera_motion;
        scene.fps = p.fps;
        scene.play_start = p.range.0;
        scene.play_end = p.range.1;
        scene.stage.background = p.background;
        scene.light.color = p.light.0;
        scene.light.direction = p.light.1;
        scene.physics_cfg = p.physics.clone();
        scene.sources = p.sources.clone();
        // Object groups: make them first, then link parents and members by position.
        let gids: Vec<u64> = p.groups.iter().map(|g| scene.new_group(&g.name, None)).collect();
        for (k, g) in p.groups.iter().enumerate() {
            let gi = scene.group_index(gids[k]).unwrap_or(0);
            scene.groups[gi].parent = g.parent.and_then(|pi| gids.get(pi).copied());
            scene.groups[gi].visible = g.visible;
            scene.groups[gi].open = g.open;
        }
        for (i, g) in placed {
            if let Some(m) = scene.models.get_mut(i) {
                m.group = g.and_then(|gi| gids.get(gi).copied());
            }
        }
        scene.sync_groups();
        scene.audio_scrub = p.audio_scrub;
        if let Some((a, off)) = p.audio {
            if scene.load_path(&a).is_ok() {
                if let Some(t) = scene.audio.as_mut() {
                    t.offset_frames = off;
                    t.volume = p.audio_volume.clamp(0.0, 2.0);
                    t.muted = p.audio_muted;
                }
            }
        }
        self.scene = scene;
        self.graph = p.effects;
        self.grade = p.grade;
        self.project_path = Some(path.to_path_buf());
        self.undo.clear();
        self.redo.clear();
        self.sel_bone = None;
        self.home = false;
        self.mark_saved();
        self.add_recent(path);
        let auto = crate::project::autosave_path(Some(path));
        let newer = |a: &std::path::Path, b: &std::path::Path| match (std::fs::metadata(a).and_then(|m| m.modified()), std::fs::metadata(b).and_then(|m| m.modified())) {
            (Ok(x), Ok(y)) => x > y,
            _ => false,
        };
        if problems.is_empty() && auto != path && newer(&auto, path) {
            Ok(format!("Opened project {}. There's a newer autosave: File → Open last autosave.", path.display()))
        } else if problems.is_empty() {
            Ok(format!("Opened project {}", path.display()))
        } else {
            Err(format!("Opened with problems: {}", problems.join("; ")))
        }
    }

    pub fn toggle_play(&mut self) {
        self.scene.playing = !self.scene.playing;
        if self.scene.playing {
            if self.scene.frame >= self.scene.play_end as f32 {
                self.scene.set_frame(self.scene.play_start as f32);
            }
            self.start_audio();
        } else {
            self.audio.stop();
        }
    }

    pub fn start_audio(&mut self) {
        self.audio_seek_rev = self.scene.seek_rev;
        self.audio_seek_at = None;
        if let Some(a) = &self.scene.audio {
            let t = (self.scene.frame - a.offset_frames as f32) / self.scene.fps;
            let p = a.path.clone();
            self.audio.play(&p, t);
        }
    }

    pub fn stop(&mut self) {
        self.scene.playing = false;
        self.audio.stop();
        self.scene.set_frame(self.scene.play_start as f32);
        self.scene.force_reset_physics();
    }

    pub fn scene_time(&self) -> f32 {
        self.scene.frame / self.scene.fps + self.wall_extra
    }

    /// Render the stage into the viewport texture at the given pixel size.
    pub fn render_viewport(&mut self, frame: &mut eframe::Frame, w: i32, h: i32) -> Option<egui::TextureId> {
        let gl = frame.gl()?.clone();
        let r = self.renderer.as_mut()?;
        let fp = FrameParams {
            scene: &self.scene,
            graph: &self.graph,
            grade: &self.grade,
            width: w,
            height: h,
            dt: self.last_render_dt,
            time: self.scene.frame / self.scene.fps + self.wall_extra,
            msaa: self.scene.stage.msaa,
            show_floor: self.scene.stage.show_floor,
            slot: 0,
            proj: self.view_proj,
        };
        let tex = r.render(&gl, &fp);
        for m in &mut self.scene.models {
            m.morph_dirty = false;
        }
        self.hist_tick += 1;
        if self.ws == Workspace::Color && self.hist_tick % 12 == 0 {
            let (_, _, px) = r.read_output(&gl);
            self.histogram = histogram(&px);
        }
        match self.viewport_tex {
            Some((t, id)) if t == tex => Some(id),
            _ => {
                let id = frame.register_native_glow_texture(tex);
                self.viewport_tex = Some((tex, id));
                Some(id)
            }
        }
    }

    /// Render the current frame offline (separate buffers) and return RGBA pixels.
    pub fn render_offline(&mut self, gl: &Arc<glow::Context>, w: u32, h: u32, dt: f32, msaa: bool) -> Option<(u32, u32, Vec<u8>)> {
        let r = self.renderer.as_mut()?;
        let fp = FrameParams {
            scene: &self.scene,
            graph: &self.graph,
            grade: &self.grade,
            width: w as i32,
            height: h as i32,
            dt,
            time: self.scene.frame / self.scene.fps,
            msaa,
            show_floor: self.scene.stage.show_floor,
            slot: 1,
            proj: None,
        };
        r.render(gl, &fp);
        for m in &mut self.scene.models {
            m.morph_dirty = false;
        }
        Some(r.read_output(gl))
    }

    pub fn start_render_job(&mut self) {
        self.render_settings.anim_fps = self.scene.fps;
        let s = self.render_settings.clone();
        let audio = self.scene.audio.as_ref().filter(|a| a.gain() > 0.0).map(|a| (a.path.clone(), -(a.offset_frames as f32) / self.scene.fps, a.gain()));
        match RenderJob::start(s, audio) {
            Ok(j) => {
                self.scene.playing = false;
                self.audio.stop();
                self.scene.frame = j.settings.start as f32;
                self.scene.force_reset_physics();
                self.set_status(format!("Rendering {} frames…", j.total()));
                self.job = Some(j);
            }
            Err(e) => {
                if e.contains("ffmpeg") {
                    self.show_video_help = true;
                }
                self.set_error(e)
            }
        }
    }

    fn pump_render_job(&mut self, frame: &mut eframe::Frame) {
        let Some(gl) = frame.gl().cloned() else { return };
        let t0 = Instant::now();
        while let Some(job) = self.job.as_mut() {
            if job.done {
                break;
            }
            // Video frame k sits at its own moment of the animation; the animation's frame rate
            // stays as it is (changing it would speed the dance up or slow it down).
            let f = job.next;
            let tl = job.timeline_frame(f);
            let (w, h, msaa, fps) = (job.settings.width, job.settings.height, job.settings.msaa, job.settings.fps);
            self.scene.frame = tl;
            self.scene.evaluate();
            let ft = Instant::now();
            let px = self.render_offline(&gl, w, h, 1.0 / fps, msaa);
            let job = self.job.as_mut().unwrap();
            job.last_frame_ms = ft.elapsed().as_secs_f32() * 1000.0;
            if let Some((w, h, px)) = px {
                job.push_frame(f, w, h, px);
            }
            job.next += 1;
            if let Some(e) = job.error.clone() {
                job.cancel();
                self.job = None;
                self.set_error(e);
                break;
            }
            if job.next >= job.total() {
                let res = job.finish();
                let open = job.settings.open_when_done.then(|| {
                    let p = job.settings.path.clone();
                    if p.is_dir() { p } else { p.parent().map(|d| d.to_path_buf()).unwrap_or(p) }
                });
                self.job = None;
                match res {
                    Ok(m) => {
                        if let Some(d) = open {
                            crate::ui::links::open_external(d.as_os_str());
                        }
                        self.set_status(m)
                    }
                    Err(e) => self.set_error(e),
                }
                break;
            }
            if t0.elapsed().as_millis() > 45 {
                break;
            }
        }
    }

    pub fn render_still_dialog(&mut self, frame: &mut eframe::Frame) {
        if let Some(p) = rfd::FileDialog::new().set_file_name("still.png").add_filter("PNG", &["png"]).save_file() {
            let res = self.render_still(frame, &p);
            match res {
                Ok(m) => self.set_status(m),
                Err(e) => self.set_error(e),
            }
        }
    }

    pub fn render_still(&mut self, frame: &mut eframe::Frame, p: &std::path::Path) -> Result<String, String> {
        let gl = frame.gl().cloned().ok_or("no GPU")?;
        let (w, h) = (self.render_settings.width, self.render_settings.height);
        let (w, h, px) = self.render_offline(&gl, w, h, 0.0, true).ok_or("renderer unavailable")?;
        image::save_buffer(p, &px, w, h, image::ColorType::Rgba8).map_err(|e| e.to_string())?;
        Ok(format!("Saved {}x{} image to {}", w, h, p.display()))
    }

    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.egui_wants_keyboard_input() {
            return;
        }
        use egui::{Key, Modifiers};
        let mut ev = Vec::new();
        ctx.input(|i| {
            for e in &i.events {
                if let egui::Event::Key { key, pressed: true, modifiers, .. } = e {
                    ev.push((*key, *modifiers));
                }
            }
        });
        for (key, m) in ev {
            let cmd = m.command;
            match key {
                Key::Space => self.toggle_play(),
                Key::ArrowRight if !cmd => {
                    let f = (self.scene.frame.floor() + 1.0).max(0.0);
                    self.scene.set_frame(f);
                }
                Key::ArrowLeft if !cmd => {
                    let f = (self.scene.frame.ceil() - 1.0).max(0.0);
                    self.scene.set_frame(f);
                }
                Key::ArrowUp => self.jump_key(true),
                Key::ArrowDown => self.jump_key(false),
                Key::Home => self.scene.set_frame(self.scene.play_start as f32),
                Key::End => self.scene.set_frame(self.scene.end_frame() as f32),
                Key::Enter => self.register_selected(),
                // Over the Physics window, undo / redo apply to physics edits.
                Key::Z if cmd && self.phys_hovered(ctx) => {
                    if m.shift {
                        self.phys_redo()
                    } else {
                        self.phys_undo()
                    }
                }
                Key::Y if cmd && self.phys_hovered(ctx) => self.phys_redo(),
                Key::Z if cmd && m.shift => self.do_redo(),
                Key::Z if cmd => self.do_undo(),
                Key::Y if cmd => self.do_redo(),
                Key::S if cmd => self.save_project_dialog(m.shift),
                Key::O if cmd => self.guard_unsaved(crate::ui::home::ConfirmAction::OpenProjectDialog, "Open another project"),
                Key::C if cmd => self.copy_keys_or_pose(),
                Key::V if cmd => self.paste_keys_or_pose(m.shift),
                Key::G if cmd && self.ws == Workspace::Effects => {
                    if m.shift {
                        if let Some(g) = self.nodes.selected.and_then(|s| self.graph.group_of(s).map(|g| g.id)) {
                            self.graph.ungroup(g);
                        }
                    } else {
                        self.group_selected();
                    }
                }
                Key::A if cmd && self.ws == Workspace::Effects => {
                    self.nodes.multi = self.graph.nodes.iter().map(|n| n.id).collect();
                }
                Key::Delete | Key::Backspace => {
                    if self.ws == Workspace::Effects {
                        if self.nodes.multi.is_empty() {
                            if let Some(id) = self.nodes.selected {
                                self.nodes.multi.push(id);
                            }
                        }
                        self.delete_selected_nodes();
                    } else {
                        self.delete_selected_keys();
                    }
                }
                // Blender-style selection and resets (Animation workspace).
                Key::A if !cmd && m.alt && self.ws == Workspace::Animation => self.select_op(crate::ui::animtools::SelOp::None),
                Key::A if !cmd && !m.alt && self.ws == Workspace::Animation => self.select_op(crate::ui::animtools::SelOp::All),
                Key::I if cmd && self.ws == Workspace::Animation => self.select_op(crate::ui::animtools::SelOp::Invert),
                Key::R if m.alt => self.reset_selected(false, true),
                Key::G if m.alt => self.reset_selected(true, false),
                Key::R => self.gizmo = GizmoMode::Rotate,
                Key::T | Key::G => self.gizmo = GizmoMode::Move,
                Key::K => self.auto_key = !self.auto_key,
                Key::F1 => self.ws = Workspace::Animation,
                Key::F2 => self.ws = Workspace::Effects,
                Key::F3 => self.ws = Workspace::Color,
                Key::F4 => self.ws = Workspace::Render,
                Key::F5 => self.ws = Workspace::Graph,
                _ => {}
            }
            let _ = Modifiers::NONE;
        }
    }

    fn top_bar(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        egui::Panel::top("top")
            .exact_size(38.0)
            .frame(egui::Frame::new().fill(theme::BG0).inner_margin(egui::Margin::symmetric(10, 5)))
            .show(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    let (r, _) = ui.allocate_exact_size(egui::vec2(18.0, 18.0), egui::Sense::hover());
                    let p = ui.painter();
                    for k in 0..8 {
                        let a = k as f32 / 8.0 * std::f32::consts::TAU;
                        let d = egui::vec2(a.cos(), a.sin());
                        p.line_segment([r.center() + d * 3.0, r.center() + d * 9.0], egui::Stroke::new(2.0, if k % 2 == 0 { theme::ACCENT } else { theme::KEY }));
                    }
                    if ui.add(egui::Button::new(RichText::new("Hanabi Studio").strong().size(15.0).color(Color32::WHITE)).frame(false)).on_hover_text("Welcome screen and recent projects").clicked() {
                        self.guard_unsaved(crate::ui::home::ConfirmAction::GoHome, "Leave this project");
                    }
                    let pname = self.project_path.as_ref().and_then(|p| p.file_stem()).map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "Untitled".into());
                    ui.label(RichText::new(format!("{pname}{}", if self.dirty { " •" } else { "" })).size(12.0).color(theme::TEXT_DIM))
                        .on_hover_text(if self.dirty { "Unsaved changes (Ctrl+S to save)" } else { "All changes saved" });
                    ui.add_space(8.0);
                    self.menus(ui, frame);
                    ui.add_space(16.0);
                    // workspace tabs
                    for w in Workspace::ALL {
                        let sel = self.ws == w;
                        let text = RichText::new(format!("{}  {}", w.icon(), w.label())).size(13.5).color(if sel { Color32::WHITE } else { theme::TEXT_DIM });
                        let b = egui::Button::new(text)
                            .fill(if sel { theme::ACCENT.linear_multiply(0.35) } else { Color32::TRANSPARENT })
                            .stroke(if sel { egui::Stroke::new(1.0, theme::ACCENT) } else { egui::Stroke::NONE })
                            .corner_radius(egui::CornerRadius::same(14))
                            .min_size(egui::vec2(104.0, 26.0));
                        if ui.add(b).clicked() {
                            self.ws = w;
                        }
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let mcp_on = self.mcp.as_ref().and_then(|m| m.port).is_some();
                        let mcp_txt = RichText::new(if mcp_on { "● MCP" } else { "○ MCP" }).size(12.0).color(if mcp_on { Color32::from_rgb(110, 220, 140) } else { theme::TEXT_DIM });
                        if ui.add(egui::Button::new(mcp_txt).frame(false)).on_hover_text("AI assistant connection (Model Context Protocol)").clicked() {
                            self.show_mcp = true;
                        }
                        ui.label(RichText::new(format!("{:.0} fps", self.fps)).size(12.0).color(theme::TEXT_DIM).monospace());
                        if let Some(r) = &self.renderer {
                            ui.label(RichText::new(format!("{:.1} ms GPU · {} draws · {}k tris", r.stats.gpu_ms, r.stats.draw_calls, r.stats.triangles / 1000)).size(11.5).color(theme::TEXT_DIM));
                        }
                    });
                });
            });
    }

    fn menus(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        ui.menu_button("File", |ui| {
            if ui.button("Open project…  (Ctrl+O)").clicked() {
                ui.close();
                self.load_dialog("project");
            }
            ui.separator();
            for (k, l) in [
                ("model", "Load model (.pmx / .pmd)…"),
                ("accessory", "Load accessory / stage (.x)…"),
                ("motion", "Load motion (.vmd / .hmo)…"),
                ("pose", "Load pose (.vpd)…"),
                ("audio", "Load audio (.wav)…"),
            ] {
                if ui.button(l).clicked() {
                    ui.close();
                    self.load_dialog(k);
                }
            }
            if ui.button("Open any supported file…").clicked() {
                ui.close();
                self.open_dialog();
            }
            ui.separator();
            if ui.button("Save project  (Ctrl+S)").clicked() {
                ui.close();
                self.save_project_dialog(false);
            }
            if ui.button("Save project as…").clicked() {
                ui.close();
                self.save_project_dialog(true);
            }
            let auto = crate::project::autosave_path(self.project_path.as_deref());
            if ui.add_enabled(auto.exists(), egui::Button::new("Open last autosave…")).on_hover_text("Projects are backed up automatically every few minutes while they have unsaved changes").clicked() {
                ui.close();
                self.open_autosave(auto);
            }
            ui.separator();
            if ui.button("Export model motion (.vmd / .hmo)…").clicked() {
                ui.close();
                self.save_motion_dialog(false);
            }
            if ui.button("Export camera & light motion…").clicked() {
                ui.close();
                self.save_motion_dialog(true);
            }
            if ui.button("Export current pose (.vpd)…").clicked() {
                ui.close();
                self.save_pose_dialog();
            }
            ui.separator();
            if ui.button("Render still image…").clicked() {
                ui.close();
                self.render_still_dialog(frame);
            }
            if ui.button("New project").clicked() {
                ui.close();
                self.guard_unsaved(crate::ui::home::ConfirmAction::NewProject, "Start a new project");
            }
            if ui.button("Welcome screen / recent projects").clicked() {
                ui.close();
                self.guard_unsaved(crate::ui::home::ConfirmAction::GoHome, "Leave this project");
            }
            if ui.button("Open sample project").clicked() {
                ui.close();
                self.guard_unsaved(crate::ui::home::ConfirmAction::OpenSample, "Open the sample");
            }
            if ui.button("Add demo mannequin").clicked() {
                ui.close();
                self.scene.add_model(crate::demo::mannequin());
            }
        });
        ui.menu_button("Edit", |ui| {
            if ui.add_enabled(!self.undo.is_empty(), egui::Button::new("Undo  (Ctrl+Z)")).clicked() {
                ui.close();
                self.do_undo();
            }
            if ui.add_enabled(!self.redo.is_empty(), egui::Button::new("Redo  (Ctrl+Y)")).clicked() {
                ui.close();
                self.do_redo();
            }
            ui.separator();
            if ui.button("Copy keys / pose  (Ctrl+C)").clicked() {
                ui.close();
                self.copy_keys_or_pose();
            }
            if ui.button("Paste  (Ctrl+V)").clicked() {
                ui.close();
                self.paste_keys_or_pose(false);
            }
            if ui.button("Paste mirrored  (Ctrl+Shift+V)").clicked() {
                ui.close();
                self.paste_keys_or_pose(true);
            }
            if ui.button("Delete selected keys  (Del)").clicked() {
                ui.close();
                self.delete_selected_keys();
            }
        });
        ui.menu_button("View", |ui| {
            let st = &mut self.scene.stage;
            ui.checkbox(&mut st.show_bones, "Bones");
            ui.checkbox(&mut st.show_rigid, "Physics shapes");
            ui.checkbox(&mut st.show_floor, "Floor");
            ui.checkbox(&mut st.show_grid, "Grid");
            ui.checkbox(&mut st.edges, "Outlines (edges)");
            ui.checkbox(&mut st.self_shadow, "Self shadow");
            ui.checkbox(&mut st.ground_shadow, "Ground shadow");
            ui.checkbox(&mut st.msaa, "Anti-aliasing (4×)");
            ui.separator();
            if let Some(r) = self.renderer.as_mut() {
                ui.checkbox(&mut r.front_ccw, "Front faces counter-clockwise");
            }
        });
        ui.menu_button("Help", |ui| {
            if ui.button("Shortcuts & tips").clicked() {
                ui.close();
                self.show_help = true;
            }
            if ui.button("AI connection (MCP)").clicked() {
                ui.close();
                self.show_mcp = true;
            }
            if ui.button("Set up video export (ffmpeg)…").clicked() {
                ui.close();
                self.show_video_help = true;
            }
            if ui.button("Credits of loaded files…").clicked() {
                ui.close();
                self.show_credits = true;
            }
        });
    }

    fn status_bar(&mut self, ui: &mut egui::Ui) {
        egui::Panel::bottom("status")
            .exact_size(24.0)
            .frame(egui::Frame::new().fill(theme::BG0).inner_margin(egui::Margin::symmetric(10, 3)))
            .show(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    let c = if self.status_err { Color32::from_rgb(255, 120, 110) } else { theme::TEXT_DIM };
                    ui.label(RichText::new(&self.status).size(12.0).color(c));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let n: usize = self.scene.models.iter().map(|m| m.physics.as_ref().map(|p| p.body_count()).unwrap_or(0)).sum();
                        let parts = self.renderer.as_ref().map(|r| r.stats.particles).unwrap_or(0);
                        ui.label(RichText::new(format!(
                            "Frame {:.0} / {}  ·  {} models  ·  {} rigid bodies  ·  {} particles  ·  effect: {}",
                            self.scene.frame,
                            self.scene.end_frame(),
                            self.scene.models.len(),
                            n,
                            parts,
                            self.graph.preset_name
                        )).size(12.0).color(theme::TEXT_DIM));
                    });
                });
            });
    }

    fn dialogs(&mut self, ctx: &egui::Context) {
        self.physics_window(ctx);
        self.video_help_window(ctx);
        self.credits_window(ctx);
        if self.show_help {
            let mut open = true;
            egui::Window::new("Shortcuts & tips").open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
                egui::Grid::new("keys").striped(true).show(ui, |ui| {
                    for (k, d) in [
                        ("Space", "Play / pause"),
                        ("← / →", "Previous / next frame"),
                        ("↑ / ↓", "Next / previous key of the selected bone"),
                        ("Enter", "Register a key for the selected bone or morph"),
                        ("R / T", "Rotate / move gizmo"),
                        ("Ctrl/Shift + click", "Select several bones (view or list)"),
                        ("A / Alt+A / Ctrl+I", "Select all / none / invert bones"),
                        ("Alt+R / Alt+G", "Reset rotation / position of the selected bones"),
                        ("Double-click", "Reset a slider (or a value's label) to its default"),
                        ("K", "Toggle auto-key"),
                        ("Ctrl+C / Ctrl+V", "Copy / paste keys or pose"),
                        ("Ctrl+Shift+V", "Paste mirrored (left ↔ right)"),
                        ("Delete", "Delete selected keys (or node in Effects)"),
                        ("Ctrl+Z / Ctrl+Y", "Undo / redo"),
                        ("F1–F4", "Animation / Effects / Color / Render"),
                        ("Right drag", "Orbit camera"),
                        ("Middle drag / Shift+right", "Pan camera"),
                        ("Wheel", "Zoom"),
                    ] {
                        ui.label(RichText::new(k).monospace().color(theme::KEY));
                        ui.label(d);
                        ui.end_row();
                    }
                });
                ui.separator();
                ui.label("Drop .pmx, .pmd, .x, .vmd, .vpd, .wav or .hproj files onto the window to open them.");
                ui.label("Pose edits that are not registered as keys are discarded when you change frame (turn on auto-key to key every edit).");
            });
            self.show_help = open;
        }
        if self.show_mcp {
            let mut open = true;
            let port = self.mcp.as_ref().and_then(|m| m.port);
            let calls = self.mcp.as_ref().map(|m| m.calls).unwrap_or(0);
            egui::Window::new("AI connection (MCP)").open(&mut open).collapsible(false).default_width(460.0).show(ctx, |ui| {
                match port {
                    Some(p) => {
                        ui.label(RichText::new(format!("● Listening on http://127.0.0.1:{p}/mcp")).color(Color32::from_rgb(110, 220, 140)));
                    }
                    None => {
                        ui.label(RichText::new("HTTP server is off (port busy or disabled).").color(Color32::from_rgb(255, 140, 120)));
                    }
                }
                ui.label(format!("Tool calls handled this session: {calls}"));
                ui.separator();
                ui.label("Add this to your AI client's MCP settings to let it start and drive the studio:");
                let exe = std::env::current_exe().map(|p| p.display().to_string()).unwrap_or_else(|_| "hanabi-studio".into());
                let cfg = format!("{{\n  \"mcpServers\": {{\n    \"hanabi-studio\": {{\n      \"command\": \"{}\",\n      \"args\": [\"--mcp-stdio\"]\n    }}\n  }}\n}}", exe.replace('\\', "\\\\"));
                let mut c = cfg.clone();
                ui.add(egui::TextEdit::multiline(&mut c).code_editor().desired_rows(8).desired_width(f32::INFINITY));
                if ui.button("Copy settings").clicked() {
                    ui.ctx().copy_text(cfg);
                }
                ui.label(RichText::new("Or connect to the running app over HTTP at the address above.").color(theme::TEXT_DIM));
                ui.separator();
                ui.label(RichText::new("Claude Code").strong());
                ui.label(RichText::new("Run one of these once in a terminal, then ask Claude things like “load my model and dance, then render a 1080p video”.").size(11.5).color(theme::TEXT_DIM));
                let quoted = if exe.contains(' ') { format!("\"{exe}\"") } else { exe.clone() };
                let stdio_cmd = format!("claude mcp add hanabi-studio -- {quoted} --mcp-stdio");
                let http_cmd = format!("claude mcp add --transport http hanabi-studio http://127.0.0.1:{}/mcp", port.unwrap_or(7860));
                for (label, cmd) in [("Claude starts the studio:", stdio_cmd), ("Use the studio that's already open:", http_cmd)] {
                    ui.label(RichText::new(label).size(11.5));
                    ui.horizontal(|ui| {
                        ui.add(egui::Label::new(RichText::new(&cmd).monospace().size(11.0).color(theme::KEY)).wrap());
                        if ui.small_button("Copy").clicked() {
                            ui.ctx().copy_text(cmd.clone());
                        }
                    });
                }
                ui.collapsing("Available tools", |ui| {
                    if let Some(arr) = crate::mcp::tools().as_array() {
                        for t in arr {
                            ui.label(RichText::new(t["name"].as_str().unwrap_or("")).monospace().color(theme::KEY));
                            ui.label(RichText::new(t["description"].as_str().unwrap_or("")).size(11.5).color(theme::TEXT_DIM));
                        }
                    }
                });
            });
            self.show_mcp = open;
        }
        if let Some(e) = self.render_error.clone() {
            egui::Window::new("Graphics problem").collapsible(false).show(ctx, |ui| {
                ui.label("The 3D view could not start. Your graphics driver needs OpenGL 3.3.");
                ui.label(RichText::new(e).monospace().size(11.0));
            });
        }
    }
}

pub fn histogram(px: &[u8]) -> Vec<[f32; 3]> {
    let mut h = vec![[0u32; 3]; 64];
    for p in px.chunks_exact(4).step_by(7) {
        for c in 0..3 {
            h[(p[c] as usize) >> 2][c] += 1;
        }
    }
    let max = h.iter().flat_map(|b| b.iter()).copied().max().unwrap_or(1).max(1) as f32;
    h.iter().map(|b| [b[0] as f32 / max, b[1] as f32 / max, b[2] as f32 / max]).collect()
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let now = Instant::now();
        let dt = (now - self.last_tick).as_secs_f32().min(0.25);
        self.last_tick = now;
        self.last_render_dt = dt;
        self.fps = self.fps * 0.92 + (1.0 / dt.max(1e-4)) * 0.08;
        self.frame_count += 1;

        // AI tool calls
        self.process_mcp(frame);

        // Dropped files
        let dropped: Vec<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).collect());
        for p in dropped {
            self.open_path(&p);
        }
        self.handle_shortcuts(&ctx);

        if self.job.is_some() {
            self.pump_render_job(frame);
        } else {
            if !self.scene.playing {
                self.wall_extra += dt;
            }
            let was_playing = self.scene.playing;
            self.scene.update(dt);
            if was_playing && !self.scene.playing {
                self.audio.stop();
            }
            // Volume follows the slider live; a new start frame restarts the song in place.
            crate::audio::set_volume(self.scene.audio.as_ref().map(|a| a.gain()).unwrap_or(1.0));
            let off = self.scene.audio.as_ref().map(|a| a.offset_frames).unwrap_or(0);
            if off != self.audio_seen_offset {
                self.audio_seen_offset = off;
                self.scene.seek_rev += 1;
            }
            // The playhead jumped while playing (timeline click, frame keys, loop restart):
            // move the song there too. While scrubbing, wait until the playhead settles.
            if self.scene.seek_rev != self.audio_seek_rev {
                self.audio_seek_rev = self.scene.seek_rev;
                if self.scene.playing {
                    self.audio.stop();
                    self.audio_seek_at = Some(Instant::now());
                } else if self.scene.audio_scrub && self.screenshot.is_none() {
                    // Play WAV with frame: hear the slice of song under the frame you moved to.
                    if let Some(a) = self.scene.audio.as_ref().filter(|a| a.gain() > 0.0) {
                        let fps = self.scene.fps.max(1.0);
                        let t = (self.scene.frame.round() - a.offset_frames as f32) / fps;
                        let p = a.path.clone();
                        self.audio.play_snippet(&p, t, (1.0 / fps).max(0.09));
                    }
                }
            }
            if let Some(t) = self.audio_seek_at {
                if !self.scene.playing {
                    self.audio_seek_at = None;
                } else if t.elapsed().as_secs_f32() >= 0.12 {
                    self.start_audio();
                }
            }
        }

        // Closing the window with unsaved changes asks first.
        if ctx.input(|i| i.viewport().close_requested()) && !self.allow_close && self.screenshot.is_none() {
            self.refresh_dirty(true);
            if self.dirty {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.ask("Quit Hanabi Studio?", "Your project has changes that aren't saved. Quit anyway?", "Quit without saving", crate::ui::home::ConfirmAction::Quit);
            }
        }
        if self.ctx_close {
            self.ctx_close = false;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        self.refresh_dirty(false);
        self.autosave_tick();
        self.take_pending_thumbnail(frame);
        if self.home {
            self.status_bar(ui);
            self.welcome_ui(ui);
            self.dialogs(&ctx);
            self.confirm_window(&ctx);
            self.screenshot_tick(&ctx);
            ctx.request_repaint();
            return;
        }
        self.top_bar(ui, frame);
        self.status_bar(ui);
        match self.ws {
            Workspace::Animation => self.animation_ws(ui, frame),
            Workspace::Effects => self.effects_ws(ui, frame),
            Workspace::Color => self.color_ws(ui, frame),
            Workspace::Render => self.render_ws(ui, frame),
            Workspace::Graph => self.graph_ws(ui, frame),
        }
        self.dialogs(&ctx);
        self.confirm_window(&ctx);
        self.screenshot_tick(&ctx);
        ctx.request_repaint();
    }

    /// Runs even when the window is hidden: keep screenshot mode moving so it always finishes.
    fn logic(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.logic_impl(ctx, frame);
    }

    fn on_exit(&mut self, gl: Option<&glow::Context>) {
        self.on_exit_impl(gl);
    }
}

impl App {
    /// `--screenshot`: after about 120 frames, save the window and quit.
    pub fn screenshot_tick(&mut self, ctx: &egui::Context) {
        if let Some(s) = self.screenshot.as_mut() {
            if !s.requested && self.frame_count >= 120 {
                s.requested = true;
                s.requested_at = Some(Instant::now());
                let path = s.path.clone();
                ctx.request_screenshot(move |img| {
                    let [w, h] = img.size;
                    let mut px = Vec::with_capacity(w * h * 4);
                    for c in &img.pixels {
                        px.extend_from_slice(&[c.r(), c.g(), c.b(), 255]);
                    }
                    if let Some(dir) = path.parent() {
                        let _ = std::fs::create_dir_all(dir);
                    }
                    match image::save_buffer(&path, &px, w as u32, h as u32, image::ColorType::Rgba8) {
                        Ok(_) => eprintln!("saved screenshot to {}", path.display()),
                        Err(e) => eprintln!("screenshot failed: {e}"),
                    }
                    std::process::exit(0);
                });
            }
        }
        ctx.request_repaint();
    }

    /// Runs even when the window is hidden: keep screenshot mode moving so it always finishes.
    pub fn logic_impl(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        if let Some(t) = self.quit_at {
            ctx.request_repaint();
            if Instant::now() >= t {
                std::process::exit(0);
            }
        }
        let Some(s) = self.screenshot.as_ref() else { return };
        ctx.send_viewport_cmd(egui::ViewportCommand::RequestPaintWhileHidden);
        ctx.request_repaint();
        let stuck = s.started.elapsed().as_secs_f32() > 25.0 || s.requested_at.map(|t| t.elapsed().as_secs_f32() > 8.0).unwrap_or(false);
        if !stuck {
            return;
        }
        // The window never drew (hidden or no compositor): save an offscreen render instead.
        let path = s.path.clone();
        if let Some(gl) = frame.gl().cloned() {
            self.scene.evaluate();
            if let Some((w, h, px)) = self.render_offline(&gl, 1280, 720, 1.0 / 30.0, true) {
                if let Some(dir) = path.parent() {
                    let _ = std::fs::create_dir_all(dir);
                }
                match image::save_buffer(&path, &px, w, h, image::ColorType::Rgba8) {
                    Ok(_) => eprintln!("saved offscreen screenshot to {}", path.display()),
                    Err(e) => eprintln!("screenshot failed: {e}"),
                }
            }
        }
        std::process::exit(0);
    }

    pub fn on_exit_impl(&mut self, _gl: Option<&glow::Context>) {
        self.audio.stop();
        if let Some(j) = self.job.as_mut() {
            j.cancel();
        }
    }
}
