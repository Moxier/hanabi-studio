//! The stage: models, accessories, camera, light, audio and the current frame.

use crate::formats::{self, FileKind};
use crate::model::*;
use crate::motion::*;
use crate::physics::Physics;
use crate::skeleton::Skeleton;
use glam::{EulerRot, Mat4, Quat, Vec3, Vec4};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct MaterialState {
    pub diffuse: Vec4,
    pub specular: Vec3,
    pub specular_power: f32,
    pub ambient: Vec3,
    pub edge_color: Vec4,
    pub edge_size: f32,
    pub texture_tint: Vec4,
}

#[derive(Clone, Debug)]
pub struct ObjGroup {
    pub id: u64,
    pub name: String,
    pub parent: Option<u64>,
    pub visible: bool,
    pub open: bool,
}

pub struct ModelInstance {
    pub id: u64,
    pub data: Arc<ModelData>,
    pub skel: Skeleton,
    pub physics: Option<Physics>,
    pub motion: Motion,
    pub visible: bool,
    pub physics_enabled: bool,
    pub edge_width: f32,
    /// Unregistered pose edits (discarded on frame change, like the classic workflow).
    pub pose_edit: HashMap<usize, (Vec3, Quat)>,
    pub morph_edit: HashMap<usize, f32>,
    pub morph_weights: Vec<f32>,
    pub morph_offsets: Vec<Vec3>,
    pub uv_offsets: Vec<glam::Vec2>,
    pub morph_dirty: bool,
    pub materials: Vec<MaterialState>,
    pub skin: Vec<Mat4>,
    /// Accessory placement (for .x props): parent model id + bone, offset transform.
    pub attach: Option<(u64, usize)>,
    pub acc_pos: Vec3,
    pub acc_rot: Vec3,
    pub acc_scale: f32,
    pub acc_opacity: f32,
    pub world: Mat4,
    last_weights: Vec<f32>,
    /// Bones moved by physics (hidden from bone overlays unless asked for).
    pub physics_bones: Vec<bool>,
    /// Physics is paused while the model is hidden and settles again when it reappears.
    physics_paused: bool,
    /// Object group this model or prop belongs to (see `Scene::groups`).
    pub group: Option<u64>,
    /// Set when a group it is in is hidden.
    pub group_hidden: bool,
}

impl ModelInstance {
    pub fn new(id: u64, data: ModelData) -> Self {
        let skel = Skeleton::new(&data);
        let nv = data.vertices.len();
        let nm = data.morphs.len();
        let materials = data
            .materials
            .iter()
            .map(|m| MaterialState {
                diffuse: m.diffuse,
                specular: m.specular,
                specular_power: m.specular_power,
                ambient: m.ambient,
                edge_color: m.edge_color,
                edge_size: m.edge_size,
                texture_tint: Vec4::ONE,
            })
            .collect();
        let physics = Physics::new(&data, &skel);
        let is_acc = data.is_accessory;
        Self {
            id,
            skel,
            physics,
            motion: Motion { model_name: data.name.clone(), ..Default::default() },
            visible: true,
            physics_enabled: true,
            edge_width: 1.0,
            pose_edit: HashMap::new(),
            morph_edit: HashMap::new(),
            morph_weights: vec![0.0; nm],
            morph_offsets: vec![Vec3::ZERO; nv],
            uv_offsets: vec![glam::Vec2::ZERO; nv],
            morph_dirty: true,
            materials,
            skin: Vec::new(),
            attach: None,
            acc_pos: Vec3::ZERO,
            acc_rot: Vec3::ZERO,
            acc_scale: if is_acc { 10.0 } else { 1.0 },
            acc_opacity: 1.0,
            world: Mat4::IDENTITY,
            last_weights: vec![-1.0; nm],
            physics_bones: {
                let mut v = vec![false; data.bones.len()];
                for rb in &data.rigid_bodies {
                    if rb.mode != crate::model::RigidMode::FollowBone && rb.bone >= 0 && (rb.bone as usize) < v.len() {
                        v[rb.bone as usize] = true;
                    }
                }
                v
            },
            data: Arc::new(data),
            physics_paused: false,
            group: None,
            group_hidden: false,
        }
    }

    pub fn name(&self) -> &str {
        &self.data.name
    }

    /// Drawn in the view: visible itself and not inside a hidden group.
    pub fn shown(&self) -> bool {
        self.visible && !self.group_hidden
    }

    pub fn is_accessory(&self) -> bool {
        self.data.is_accessory
    }

    fn apply_morphs(&mut self) {
        let d = self.data.clone();
        // Expand group/flip morphs into the flat weight list.
        let mut w = self.morph_weights.clone();
        for (i, m) in d.morphs.iter().enumerate() {
            let base = self.morph_weights[i];
            if base == 0.0 {
                continue;
            }
            match &m.kind {
                MorphKind::Group(list) => {
                    for &(mi, r) in list {
                        if mi >= 0 && (mi as usize) < w.len() && mi as usize != i {
                            w[mi as usize] += base * r;
                        }
                    }
                }
                MorphKind::Flip(list) => {
                    if let Some(&(mi, r)) = list.get(((base * list.len() as f32) as usize).min(list.len().saturating_sub(1))) {
                        if mi >= 0 && (mi as usize) < w.len() {
                            w[mi as usize] += r;
                        }
                    }
                }
                _ => {}
            }
        }
        // Bone morphs every frame
        for (i, m) in d.morphs.iter().enumerate() {
            if w[i] == 0.0 {
                continue;
            }
            if let MorphKind::Bone(list) = &m.kind {
                for &(bi, t, r) in list {
                    if bi >= 0 && (bi as usize) < self.skel.states.len() {
                        let s = &mut self.skel.states[bi as usize];
                        s.morph_pos += t * w[i];
                        s.morph_rot = Quat::IDENTITY.slerp(r, w[i]) * s.morph_rot;
                    }
                }
            }
        }
        // Vertex/uv/material morphs only when weights changed
        if w != self.last_weights || self.morph_dirty {
            for o in &mut self.morph_offsets {
                *o = Vec3::ZERO;
            }
            for o in &mut self.uv_offsets {
                *o = glam::Vec2::ZERO;
            }
            for (ms, m) in self.materials.iter_mut().zip(d.materials.iter()) {
                *ms = MaterialState {
                    diffuse: m.diffuse,
                    specular: m.specular,
                    specular_power: m.specular_power,
                    ambient: m.ambient,
                    edge_color: m.edge_color,
                    edge_size: m.edge_size,
                    texture_tint: Vec4::ONE,
                };
            }
            for (i, m) in d.morphs.iter().enumerate() {
                let wi = w[i];
                if wi == 0.0 {
                    continue;
                }
                match &m.kind {
                    MorphKind::Vertex(list) => {
                        for &(vi, off) in list {
                            if let Some(o) = self.morph_offsets.get_mut(vi as usize) {
                                *o += off * wi;
                            }
                        }
                    }
                    MorphKind::Uv(list) => {
                        for &(vi, off) in list {
                            if let Some(o) = self.uv_offsets.get_mut(vi as usize) {
                                *o += glam::Vec2::new(off.x, off.y) * wi;
                            }
                        }
                    }
                    MorphKind::Material(list) => {
                        for mo in list {
                            let targets: Vec<usize> = if mo.material < 0 {
                                (0..self.materials.len()).collect()
                            } else {
                                vec![mo.material as usize]
                            };
                            for t in targets {
                                let Some(ms) = self.materials.get_mut(t) else { continue };
                                if mo.additive {
                                    ms.diffuse += mo.diffuse * wi;
                                    ms.specular += mo.specular * wi;
                                    ms.specular_power += mo.specular_power * wi;
                                    ms.ambient += mo.ambient * wi;
                                    ms.edge_color += mo.edge_color * wi;
                                    ms.edge_size += mo.edge_size * wi;
                                    ms.texture_tint += mo.texture_tint * wi;
                                } else {
                                    let l4 = |a: Vec4, b: Vec4| a * (Vec4::ONE + (b - Vec4::ONE) * wi);
                                    let l3 = |a: Vec3, b: Vec3| a * (Vec3::ONE + (b - Vec3::ONE) * wi);
                                    ms.diffuse = l4(ms.diffuse, mo.diffuse);
                                    ms.specular = l3(ms.specular, mo.specular);
                                    ms.ambient = l3(ms.ambient, mo.ambient);
                                    ms.edge_color = l4(ms.edge_color, mo.edge_color);
                                    ms.edge_size *= 1.0 + (mo.edge_size - 1.0) * wi;
                                    ms.texture_tint = l4(ms.texture_tint, mo.texture_tint);
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            self.last_weights = w;
            self.morph_dirty = true;
        }
    }

    /// Evaluate motion + edits at `frame`. `physics`: Some(dt) steps simulation, None resets it.
    pub fn evaluate(&mut self, frame: f32, physics_step: Option<f32>, physics_on: bool, cfg: &crate::physics::PhysicsCfg, time: f32) {
        let d = self.data.clone();
        self.skel.reset_anim();
        for (i, b) in d.bones.iter().enumerate() {
            if let Some((p, r)) = self.motion.eval_bone(&b.name, frame) {
                self.skel.states[i].anim_pos = p;
                self.skel.states[i].anim_rot = r;
            }
            if let Some(&(p, r)) = self.pose_edit.get(&i) {
                self.skel.states[i].anim_pos = p;
                self.skel.states[i].anim_rot = r;
            }
        }
        for (i, m) in d.morphs.iter().enumerate() {
            self.morph_weights[i] = self.motion.eval_morph(&m.name, frame).unwrap_or(0.0);
            if let Some(&w) = self.morph_edit.get(&i) {
                self.morph_weights[i] = w;
            }
        }
        let (vis, iks) = self.motion.eval_visible(frame);
        if !self.motion.show_ik.is_empty() {
            self.visible = vis;
        }
        for (n, on) in iks {
            if let Some(i) = d.bone_index(&n) {
                self.skel.ik_enabled[i] = on;
            }
        }
        self.apply_morphs();
        self.skel.update(&d, false);
        let use_phys = physics_on && self.physics_enabled && self.shown();
        let physics_step = if self.physics_paused && use_phys { None } else { physics_step };
        self.physics_paused = !self.shown();
        if let Some(ph) = self.physics.as_mut() {
            if use_phys {
                match physics_step {
                    Some(dt) => ph.step(&mut self.skel, dt, cfg, time),
                    None => {
                        ph.reset(&self.skel);
                        // settle for a few steps so hair hangs naturally
                        for k in 0..cfg.settle_steps {
                            ph.step(&mut self.skel, 1.0 / 60.0, cfg, time - (cfg.settle_steps - k) as f32 / 60.0);
                        }
                    }
                }
                self.skel.refresh_after_physics(&d);
            }
        }
        self.skel.update(&d, true);
        self.skel.skin_matrices(&d, &mut self.skin);
    }

    pub fn bone_world(&self, i: usize) -> Mat4 {
        self.world * self.skel.states[i].global
    }

    /// Current local pose of a bone (keyframe or unregistered edit).
    pub fn bone_pose(&self, i: usize) -> (Vec3, Quat) {
        let s = &self.skel.states[i];
        (s.anim_pos, s.anim_rot)
    }

    pub fn register_bone_key(&mut self, i: usize, frame: u32, curves: [Curve; 4]) {
        let (p, r) = self.bone_pose(i);
        let name = self.data.bones[i].name.clone();
        self.motion.set_bone_key(&name, BoneKey { frame, pos: p, rot: r, curves });
        self.pose_edit.remove(&i);
    }

    pub fn register_morph_key(&mut self, i: usize, frame: u32) {
        let w = self.morph_weights[i];
        let name = self.data.morphs[i].name.clone();
        self.motion.set_morph_key(&name, MorphKey { frame, weight: w });
        self.morph_edit.remove(&i);
    }
}

#[derive(Clone, Debug)]
pub struct CameraState {
    pub target: Vec3,
    pub rot: Vec3,
    pub distance: f32,
    pub fov: f32,
    pub ortho: bool,
}

impl Default for CameraState {
    fn default() -> Self {
        Self { target: Vec3::new(0.0, 10.0, 0.0), rot: Vec3::new(0.12, 0.35, 0.0), distance: -42.0, fov: 30.0, ortho: false }
    }
}

impl CameraState {
    pub fn rotation(&self) -> Quat {
        Quat::from_euler(EulerRot::YXZ, -self.rot.y, -self.rot.x, -self.rot.z)
    }
    pub fn eye(&self) -> Vec3 {
        self.target + self.rotation() * Vec3::new(0.0, 0.0, self.distance)
    }
    /// View matrix in the scene's left-handed space, mapped to OpenGL clip space.
    pub fn view(&self) -> Mat4 {
        let eye = self.eye();
        let up = self.rotation() * Vec3::Y;
        // Flip Z so the left-handed scene renders correctly with GL's right-handed conventions.
        Mat4::look_at_rh(eye * Vec3::new(1.0, 1.0, -1.0), self.target * Vec3::new(1.0, 1.0, -1.0), up * Vec3::new(1.0, 1.0, -1.0))
            * Mat4::from_scale(Vec3::new(1.0, 1.0, -1.0))
    }
    pub fn proj(&self, aspect: f32) -> Mat4 {
        if self.ortho {
            let h = self.distance.abs() * (self.fov.to_radians() * 0.5).tan();
            Mat4::orthographic_rh_gl(-h * aspect, h * aspect, -h, h, 0.5, 4000.0)
        } else {
            Mat4::perspective_rh_gl(self.fov.to_radians().clamp(0.02, 3.0), aspect, 0.5, 4000.0)
        }
    }
    pub fn to_key(&self, frame: u32, curves: [Curve; 6]) -> CameraKey {
        CameraKey { frame, target: self.target, rot: self.rot, distance: self.distance, fov: self.fov, ortho: self.ortho, curves }
    }
    pub fn from_key(k: &CameraKey) -> Self {
        Self { target: k.target, rot: k.rot, distance: k.distance, fov: k.fov, ortho: k.ortho }
    }
}

#[derive(Clone, Debug)]
pub struct LightState {
    pub color: Vec3,
    pub direction: Vec3,
}

impl Default for LightState {
    fn default() -> Self {
        Self { color: Vec3::splat(0.6), direction: Vec3::new(-0.5, -1.0, 0.5) }
    }
}

#[derive(Clone, Debug)]
pub struct StageSettings {
    pub background: [f32; 3],
    pub show_floor: bool,
    pub show_grid: bool,
    pub ground_shadow: bool,
    pub self_shadow: bool,
    pub shadow_distance: f32,
    pub show_bones: bool,
    pub show_rigid: bool,
    /// Show bones that physics moves (hair, skirt…). Off by default to keep posing tidy.
    pub show_physics_bones: bool,
    pub physics: bool,
    pub physics_substeps: u32,
    pub edges: bool,
    pub msaa: bool,
}

impl Default for StageSettings {
    fn default() -> Self {
        Self {
            background: [0.075, 0.08, 0.1],
            show_floor: true,
            show_grid: true,
            ground_shadow: true,
            self_shadow: true,
            shadow_distance: 60.0,
            show_bones: true,
            show_rigid: false,
            show_physics_bones: false,
            physics: true,
            physics_substeps: 2,
            edges: true,
            msaa: true,
        }
    }
}

pub struct AudioTrack {
    pub path: PathBuf,
    pub info: formats::wav::Audio,
    /// The song starts at this frame (negative: the song's beginning is skipped).
    pub offset_frames: i32,
    /// 1.0 = as recorded, up to 2.0.
    pub volume: f32,
    pub muted: bool,
}

impl AudioTrack {
    /// The level the song plays and exports at.
    pub fn gain(&self) -> f32 {
        if self.muted { 0.0 } else { self.volume.clamp(0.0, 2.0) }
    }
}

pub struct Scene {
    pub models: Vec<ModelInstance>,
    pub selected: Option<usize>,
    pub camera: CameraState,
    /// Camera/light motion (scene-level keys).
    pub camera_motion: Motion,
    pub light: LightState,
    pub stage: StageSettings,
    pub frame: f32,
    pub fps: f32,
    pub playing: bool,
    pub loop_play: bool,
    pub play_start: u32,
    pub play_end: u32,
    pub audio: Option<AudioTrack>,
    pub camera_follows_motion: bool,
    pub camera_edited: bool,
    pub light_edited: bool,
    next_id: u64,
    /// Object groups, like collections: models and props can be put in groups, groups can sit
    /// inside other groups, and hiding a group hides everything in it.
    pub groups: Vec<ObjGroup>,
    /// Counts jumps of the playhead (clicks, frame keys, loop restarts) so the song can follow.
    pub seek_rev: u64,
    /// "Play WAV with frame": while stopped, moving to a frame plays that frame's slice of the song.
    pub audio_scrub: bool,
    last_eval_frame: f32,
    pub log: Vec<String>,
    /// Motion, pose and song files used in this scene (for the credits list).
    pub sources: Vec<PathBuf>,
    pub physics_cfg: crate::physics::PhysicsCfg,
}

impl Scene {
    pub fn new() -> Self {
        Self {
            models: Vec::new(),
            selected: None,
            camera: CameraState::default(),
            camera_motion: Motion::default(),
            light: LightState::default(),
            stage: StageSettings::default(),
            frame: 0.0,
            fps: 30.0,
            playing: false,
            loop_play: true,
            play_start: 0,
            play_end: 240,
            audio: None,
            camera_follows_motion: true,
            camera_edited: false,
            light_edited: false,
            next_id: 1,
            groups: Vec::new(),
            seek_rev: 0,
            audio_scrub: false,
            last_eval_frame: -1.0,
            log: Vec::new(),
            sources: Vec::new(),
            physics_cfg: Default::default(),
        }
    }

    pub fn add_model(&mut self, data: ModelData) -> usize {
        let id = self.next_id;
        self.next_id += 1;
        let mut inst = ModelInstance::new(id, data);
        let cfg = self.physics_cfg.clone();
        inst.evaluate(self.frame, None, self.stage.physics, &cfg, self.frame / self.fps);
        self.models.push(inst);
        self.selected = Some(self.models.len() - 1);
        self.last_eval_frame = -1.0;
        self.models.len() - 1
    }

    pub fn remove_model(&mut self, i: usize) {
        if i < self.models.len() {
            let id = self.models[i].id;
            self.models.remove(i);
            for m in &mut self.models {
                if matches!(m.attach, Some((pid, _)) if pid == id) {
                    m.attach = None;
                }
            }
            self.selected = if self.models.is_empty() { None } else { Some(i.min(self.models.len() - 1)) };
        }
    }

    pub fn new_group(&mut self, name: &str, parent: Option<u64>) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        let parent = parent.filter(|p| self.group_index(*p).is_some());
        self.groups.push(ObjGroup { id, name: name.to_string(), parent, visible: true, open: true });
        id
    }

    pub fn group_index(&self, id: u64) -> Option<usize> {
        self.groups.iter().position(|g| g.id == id)
    }

    /// Whether `inner` is `outer` or sits somewhere inside it.
    pub fn group_within(&self, inner: u64, outer: u64) -> bool {
        let mut cur = Some(inner);
        let mut guard = 0;
        while let Some(c) = cur {
            if c == outer {
                return true;
            }
            cur = self.group_index(c).and_then(|i| self.groups[i].parent);
            guard += 1;
            if guard > 256 {
                break;
            }
        }
        false
    }

    /// A group is shown when it and every group around it are shown.
    pub fn group_shown(&self, id: u64) -> bool {
        let mut cur = Some(id);
        let mut guard = 0;
        while let Some(c) = cur {
            let Some(i) = self.group_index(c) else { return true };
            if !self.groups[i].visible {
                return false;
            }
            cur = self.groups[i].parent;
            guard += 1;
            if guard > 256 {
                break;
            }
        }
        true
    }

    /// Bring group membership and hidden groups up to date after a change.
    pub fn sync_groups(&mut self) {
        let ids: Vec<u64> = self.groups.iter().map(|g| g.id).collect();
        for g in 0..self.groups.len() {
            let own = self.groups[g].id;
            if matches!(self.groups[g].parent, Some(p) if !ids.contains(&p) || p == own) {
                self.groups[g].parent = None;
            }
        }
        for i in 0..self.models.len() {
            if matches!(self.models[i].group, Some(g) if !ids.contains(&g)) {
                self.models[i].group = None;
            }
            let hidden = self.models[i].group.map(|g| !self.group_shown(g)).unwrap_or(false);
            self.models[i].group_hidden = hidden;
        }
    }

    /// Move a group into another group (or to the top with `None`); refuses loops.
    pub fn move_group(&mut self, id: u64, into: Option<u64>) -> bool {
        if let Some(t) = into {
            if self.group_within(t, id) {
                return false;
            }
        }
        if let Some(i) = self.group_index(id) {
            self.groups[i].parent = into;
        }
        self.sync_groups();
        true
    }

    /// Remove a group. With `keep_contents` its objects and groups move up one level;
    /// otherwise everything inside is removed from the scene too.
    pub fn delete_group(&mut self, id: u64, keep_contents: bool) {
        let Some(gi) = self.group_index(id) else { return };
        let parent = self.groups[gi].parent;
        if keep_contents {
            for g in &mut self.groups {
                if g.parent == Some(id) {
                    g.parent = parent;
                }
            }
            for m in &mut self.models {
                if m.group == Some(id) {
                    m.group = parent;
                }
            }
            self.groups.remove(gi);
        } else {
            let inside: Vec<u64> = self.groups.iter().map(|g| g.id).filter(|&g| self.group_within(g, id)).collect();
            let mut i = self.models.len();
            while i > 0 {
                i -= 1;
                if matches!(self.models[i].group, Some(g) if inside.contains(&g)) {
                    self.remove_model(i);
                }
            }
            self.groups.retain(|g| !inside.contains(&g.id));
        }
        self.sync_groups();
    }

    /// Objects inside a group, including its sub-groups.
    pub fn group_object_count(&self, id: u64) -> usize {
        self.models.iter().filter(|m| matches!(m.group, Some(g) if self.group_within(g, id))).count()
    }

    pub fn model_by_id(&self, id: u64) -> Option<usize> {
        self.models.iter().position(|m| m.id == id)
    }

    pub fn end_frame(&self) -> u32 {
        let mut e = self.camera_motion.max_frame();
        for m in &self.models {
            e = e.max(m.motion.max_frame());
        }
        if let Some(a) = &self.audio {
            e = e.max((a.info.duration * self.fps) as u32);
        }
        e
    }

    pub fn set_frame(&mut self, f: f32) {
        if (f - self.frame).abs() > 0.0001 {
            // Changing frame discards unregistered edits (classic workflow).
            for m in &mut self.models {
                m.pose_edit.clear();
                m.morph_edit.clear();
            }
            self.camera_edited = false;
            self.light_edited = false;
            self.seek_rev += 1;
        }
        self.frame = f.max(0.0);
    }

    /// Advance and evaluate everything. `dt` is wall-clock time since last update.
    pub fn update(&mut self, dt: f32) {
        if self.playing {
            let mut f = self.frame + dt * self.fps;
            if f > self.play_end as f32 {
                if self.loop_play {
                    f = self.play_start as f32;
                    self.last_eval_frame = -1.0;
                    self.seek_rev += 1;
                } else {
                    f = self.play_end as f32;
                    self.playing = false;
                }
            }
            self.frame = f;
        }
        self.evaluate();
    }

    pub fn evaluate(&mut self) {
        let frame = self.frame;
        let jumped = !self.playing && (frame - self.last_eval_frame).abs() > 1.5 || self.last_eval_frame < 0.0;
        let step = if jumped { None } else { Some(((frame - self.last_eval_frame).abs() / self.fps).clamp(1.0 / 240.0, 1.0 / 20.0)) };
        let physics = self.stage.physics;
        let substeps = self.stage.physics_substeps;
        let cfg = self.physics_cfg.clone();
        let fps = self.fps.max(1.0);
        // Models first (accessories may follow their bones).
        let still = (frame - self.last_eval_frame).abs() < 1e-4 && !self.playing;
        for m in self.models.iter_mut().filter(|m| !m.is_accessory()) {
            if let Some(p) = m.physics.as_mut() {
                p.substeps = substeps;
            }
            // When paused on the same frame, keep simulating gently so hair settles while posing.
            let s = if still { Some(1.0 / 60.0) } else { step };
            m.evaluate(frame, s, physics, &cfg, frame / fps);
        }
        let mut worlds = Vec::new();
        for (i, m) in self.models.iter().enumerate() {
            if m.is_accessory() {
                let parent = m.attach.and_then(|(pid, bi)| {
                    self.models.iter().find(|x| x.id == pid).and_then(|p| p.skel.states.get(bi).map(|s| p.world * s.global))
                });
                let local = Mat4::from_scale_rotation_translation(
                    Vec3::splat(m.acc_scale),
                    Quat::from_euler(EulerRot::YXZ, m.acc_rot.y.to_radians(), m.acc_rot.x.to_radians(), m.acc_rot.z.to_radians()),
                    m.acc_pos,
                );
                worlds.push((i, parent.unwrap_or(Mat4::IDENTITY) * local));
            }
        }
        for (i, w) in worlds {
            self.models[i].world = w;
            self.models[i].evaluate(frame, step, false, &cfg, frame / fps);
        }
        if self.camera_follows_motion && !self.camera_edited {
            if let Some(k) = self.camera_motion.eval_camera(frame) {
                self.camera = CameraState::from_key(&k);
            }
        }
        if !self.light_edited {
            if let Some((c, d)) = self.camera_motion.eval_light(frame) {
                self.light.color = c;
                self.light.direction = d;
            }
        }
        self.last_eval_frame = frame;
    }

    pub fn force_reset_physics(&mut self) {
        self.last_eval_frame = -1.0;
    }

    pub fn load_path(&mut self, path: &Path) -> Result<String, String> {
        let r = self.load_path_inner(path);
        if r.is_ok() && matches!(formats::kind_of(path), FileKind::Motion | FileKind::OwnMotion | FileKind::Pose | FileKind::Audio) && !self.sources.iter().any(|p| p == path) {
            self.sources.push(path.to_path_buf());
        }
        r
    }

    fn load_path_inner(&mut self, path: &Path) -> Result<String, String> {
        let data = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let fname = path.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        match formats::kind_of(path) {
            FileKind::Model => {
                let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
                let mut m = if ext == "pmd" { formats::pmd::read(&data)? } else { formats::pmx::read(&data)? };
                m.path = Some(path.to_path_buf());
                let msg = format!(
                    "Loaded model “{}”: {} vertices, {} bones, {} morphs, {} rigid bodies",
                    m.name,
                    m.vertices.len(),
                    m.bones.len(),
                    m.morphs.len(),
                    m.rigid_bodies.len()
                );
                self.add_model(m);
                Ok(msg)
            }
            FileKind::Accessory => {
                let mut m = formats::xfile::read(&data)?;
                m.name = fname.clone();
                m.path = Some(path.to_path_buf());
                self.add_model(m);
                Ok(format!("Loaded accessory {fname}"))
            }
            FileKind::Motion | FileKind::OwnMotion => {
                let motion = if formats::kind_of(path) == FileKind::Motion {
                    formats::vmd::read(&data)?
                } else {
                    load_own(&String::from_utf8_lossy(&data))?
                };
                let keys = motion.key_count();
                if motion.is_camera_motion() {
                    self.camera_motion.merge(&motion, 0);
                    self.camera_follows_motion = true;
                    self.force_reset_physics();
                    self.play_end = self.play_end.max(self.end_frame());
                    return Ok(format!("Loaded camera/light motion {fname} ({keys} keys)"));
                }
                let Some(sel) = self.selected.filter(|&i| !self.models[i].is_accessory()) else {
                    return Err("Select a model first, then load the motion onto it.".into());
                };
                let m = &mut self.models[sel];
                let mut motion = motion;
                motion.fit_to_model(&m.data);
                let matched = motion.bones.keys().filter(|n| m.data.bone_index(n).is_some()).count();
                let total = motion.bones.len();
                let cam = (!motion.camera.is_empty() || !motion.light.is_empty()).then(|| Motion { camera: motion.camera.clone(), light: motion.light.clone(), ..Default::default() });
                m.motion.merge_owned(motion, 0);
                if let Some(cam) = cam {
                    self.camera_motion.merge(&cam, 0);
                }
                let name = m.data.name.clone();
                self.force_reset_physics();
                self.play_end = self.play_end.max(self.end_frame());
                Ok(format!("Applied motion {fname} to “{name}”: {keys} keys, {matched}/{total} bones matched"))
            }
            FileKind::Pose => {
                let pose = formats::vpd::read(&data)?;
                let Some(sel) = self.selected else { return Err("Select a model first".into()) };
                let m = &mut self.models[sel];
                let mut n = 0;
                for (name, t, r) in &pose.bones {
                    if let Some(i) = m.data.bone_index(name) {
                        m.pose_edit.insert(i, (*t, *r));
                        n += 1;
                    }
                }
                for (name, w) in &pose.morphs {
                    if let Some(i) = m.data.morph_index(name) {
                        m.morph_edit.insert(i, *w);
                    }
                }
                Ok(format!("Applied pose {fname} ({n} bones). Register keys to keep it."))
            }
            FileKind::Audio => {
                let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
                let info = if ext == "wav" {
                    formats::wav::read(&data)?
                } else {
                    formats::wav::Audio { sample_rate: 0, channels: 0, duration: 0.0, peaks: vec![] }
                };
                let dur = info.duration;
                // Replacing the song keeps its volume and start frame.
                let (offset_frames, volume, muted) = self.audio.as_ref().map(|a| (a.offset_frames, a.volume, a.muted)).unwrap_or((0, 1.0, false));
                self.audio = Some(AudioTrack { path: path.to_path_buf(), info, offset_frames, volume, muted });
                self.play_end = self.play_end.max((dur * self.fps) as u32);
                Ok(format!("Loaded audio {fname} ({dur:.1} s)"))
            }
            _ => Err(format!("{fname}: this file type can't be opened here")),
        }
    }
}
