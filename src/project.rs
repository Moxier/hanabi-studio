//! The studio's own project format (.hproj, JSON). It references model/audio files by path
//! and stores motions, effects and grading inline.

use crate::effects::EffectGraph;
use crate::grade::ColorGrade;
use crate::motion::Motion;
use crate::scene::Scene;
use glam::Vec3;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Serialize, Deserialize)]
pub struct ProjModel {
    pub path: Option<PathBuf>,
    pub demo: bool,
    #[serde(with = "crate::motionbin")]
    pub motion: Motion,
    pub visible: bool,
    pub physics: bool,
    pub edge_width: f32,
    pub attach: Option<(usize, String)>,
    pub acc_pos: Vec3,
    pub acc_rot: Vec3,
    pub acc_scale: f32,
    pub acc_opacity: f32,
    /// Index into `Project::groups`.
    #[serde(default)]
    pub group: Option<usize>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct ProjGroup {
    pub name: String,
    pub parent: Option<usize>,
    pub visible: bool,
    pub open: bool,
}

#[derive(Serialize, Deserialize)]
pub struct Project {
    pub format: String,
    pub version: u32,
    pub models: Vec<ProjModel>,
    #[serde(with = "crate::motionbin")]
    pub camera_motion: Motion,
    pub effects: EffectGraph,
    pub grade: ColorGrade,
    pub audio: Option<(PathBuf, i32)>,
    pub fps: f32,
    pub range: (u32, u32),
    pub background: [f32; 3],
    pub light: (Vec3, Vec3),
    #[serde(default)]
    pub physics: crate::physics::PhysicsCfg,
    /// Motion, pose and song files used (for the credits list).
    #[serde(default)]
    pub sources: Vec<PathBuf>,
    #[serde(default)]
    pub groups: Vec<ProjGroup>,
    #[serde(default = "one")]
    pub audio_volume: f32,
    #[serde(default)]
    pub audio_muted: bool,
    #[serde(default)]
    pub audio_scrub: bool,
}

fn one() -> f32 {
    1.0
}

pub fn save(scene: &Scene, effects: &EffectGraph, grade: &ColorGrade) -> Result<String, String> {
    let models = scene
        .models
        .iter()
        .map(|m| ProjModel {
            path: m.data.path.clone(),
            demo: m.data.path.is_none(),
            motion: m.motion.clone(),
            visible: m.visible,
            physics: m.physics_enabled,
            edge_width: m.edge_width,
            attach: m.attach.and_then(|(id, b)| {
                let idx = scene.model_by_id(id)?;
                Some((idx, scene.models[idx].data.bones.get(b)?.name.clone()))
            }),
            acc_pos: m.acc_pos,
            acc_rot: m.acc_rot,
            acc_scale: m.acc_scale,
            acc_opacity: m.acc_opacity,
            group: m.group.and_then(|g| scene.group_index(g)),
        })
        .collect();
    let p = Project {
        format: "hanabi-project".into(),
        version: 2,
        models,
        camera_motion: scene.camera_motion.clone(),
        effects: effects.clone(),
        grade: grade.clone(),
        audio: scene.audio.as_ref().map(|a| (a.path.clone(), a.offset_frames)),
        fps: scene.fps,
        range: (scene.play_start, scene.play_end),
        background: scene.stage.background,
        light: (scene.light.color, scene.light.direction),
        physics: scene.physics_cfg.clone(),
        sources: scene.sources.clone(),
        groups: scene
            .groups
            .iter()
            .map(|g| ProjGroup { name: g.name.clone(), parent: g.parent.and_then(|p| scene.group_index(p)), visible: g.visible, open: g.open })
            .collect(),
        audio_volume: scene.audio.as_ref().map(|a| a.volume).unwrap_or(1.0),
        audio_muted: scene.audio.as_ref().map(|a| a.muted).unwrap_or(false),
        audio_scrub: scene.audio_scrub,
    };
    serde_json::to_string(&p).map_err(|e| e.to_string())
}

/// A small fingerprint of everything except keyframes, for the unsaved-changes dot.
/// Keyframe edits are counted separately (every key edit makes an undo step).
pub fn fingerprint(scene: &Scene, effects: &EffectGraph, grade: &ColorGrade) -> String {
    #[derive(Serialize)]
    struct Light<'a> {
        models: Vec<(Option<&'a std::path::Path>, bool, bool, f32, Vec3, Vec3, f32, f32, &'a str, usize)>,
        effects: &'a EffectGraph,
        grade: &'a ColorGrade,
        audio: Option<(&'a std::path::Path, i32, f32, bool)>,
        audio_scrub: bool,
        fps: f32,
        range: (u32, u32),
        background: [f32; 3],
        light: (Vec3, Vec3),
        physics: &'a crate::physics::PhysicsCfg,
        groups: Vec<(String, Option<u64>, bool)>,
        membership: Vec<Option<u64>>,
    }
    let l = Light {
        models: scene
            .models
            .iter()
            .map(|m| (m.data.path.as_deref(), m.visible, m.physics_enabled, m.edge_width, m.acc_pos, m.acc_rot, m.acc_scale, m.acc_opacity, m.data.name.as_str(), m.motion.key_count()))
            .collect(),
        effects,
        grade,
        audio: scene.audio.as_ref().map(|a| (a.path.as_path(), a.offset_frames, a.volume, a.muted)),
        audio_scrub: scene.audio_scrub,
        fps: scene.fps,
        range: (scene.play_start, scene.play_end),
        background: scene.stage.background,
        light: (scene.light.color, scene.light.direction),
        physics: &scene.physics_cfg,
        groups: scene.groups.iter().map(|g| (g.name.clone(), g.parent, g.visible)).collect(),
        membership: scene.models.iter().map(|m| m.group).collect(),
    };
    serde_json::to_string(&l).unwrap_or_default()
}

pub fn load(text: &str) -> Result<Project, String> {
    let p: Project = serde_json::from_str(text).map_err(|e| format!("not a valid project: {e}"))?;
    if p.format != "hanabi-project" {
        return Err("not a Hanabi Studio project".into());
    }
    Ok(p)
}

/// Where the automatic backup of a project goes: next to the project, or in the settings folder
/// for a project that was never saved.
pub fn autosave_path(project: Option<&std::path::Path>) -> PathBuf {
    match project {
        Some(p) => {
            let stem = p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "project".into());
            p.with_file_name(format!("{stem}.autosave.hproj"))
        }
        None => crate::ui::home::config_dir().join("autosave").join("untitled.autosave.hproj"),
    }
}

/// Write a project safely: to a temporary file first, keep the previous save as `.bak`, then
/// move the new file into place, so a crash or full disk never leaves a half-written project.
pub fn write_safely(path: &std::path::Path, text: &str, keep_backup: bool) -> Result<(), String> {
    if let Some(d) = path.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    let tmp = path.with_extension("hproj.saving");
    std::fs::write(&tmp, text).map_err(|e| format!("{}: {e}", path.display()))?;
    if keep_backup && path.exists() {
        let bak = path.with_extension("hproj.bak");
        let _ = std::fs::rename(path, &bak);
    }
    std::fs::rename(&tmp, path).map_err(|e| format!("{}: {e}", path.display()))
}
