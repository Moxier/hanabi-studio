//! Keyframe animation data, curve interpolation and evaluation.

use glam::{EulerRot, Quat, Vec3};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// One cubic easing curve stored like MMD does: control points (x1,y1) (x2,y2) in 0..=127.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Curve {
    pub x1: u8,
    pub y1: u8,
    pub x2: u8,
    pub y2: u8,
}

impl Default for Curve {
    fn default() -> Self {
        Self::LINEAR
    }
}

impl Curve {
    pub const LINEAR: Curve = Curve { x1: 20, y1: 20, x2: 107, y2: 107 };
    pub const EASE_IN_OUT: Curve = Curve { x1: 64, y1: 0, x2: 64, y2: 127 };
    pub const EASE_IN: Curve = Curve { x1: 64, y1: 0, x2: 127, y2: 127 };
    pub const EASE_OUT: Curve = Curve { x1: 0, y1: 0, x2: 64, y2: 127 };
    pub const SHARP: Curve = Curve { x1: 100, y1: 0, x2: 27, y2: 127 };
    pub const STEP: Curve = Curve { x1: 127, y1: 0, x2: 127, y2: 0 };

    pub fn presets() -> [(&'static str, Curve); 6] {
        [
            ("Linear", Curve::LINEAR),
            ("Smooth", Curve::EASE_IN_OUT),
            ("Ease in", Curve::EASE_IN),
            ("Ease out", Curve::EASE_OUT),
            ("Snappy", Curve::SHARP),
            ("Hold", Curve::STEP),
        ]
    }

    pub fn is_linear(&self) -> bool {
        self.x1 == self.y1 && self.x2 == self.y2
    }

    /// Evaluate the curve: given progress x in 0..1 return eased y in 0..1.
    pub fn eval(&self, x: f32) -> f32 {
        if self.is_linear() {
            return x;
        }
        if *self == Curve::STEP {
            return if x >= 1.0 { 1.0 } else { 0.0 };
        }
        let x1 = self.x1 as f32 / 127.0;
        let y1 = self.y1 as f32 / 127.0;
        let x2 = self.x2 as f32 / 127.0;
        let y2 = self.y2 as f32 / 127.0;
        let bez = |t: f32, a: f32, b: f32| {
            let it = 1.0 - t;
            3.0 * it * it * t * a + 3.0 * it * t * t * b + t * t * t
        };
        // Bisection on x(t) (monotonic for control points in 0..1).
        let (mut lo, mut hi) = (0.0f32, 1.0f32);
        let mut t = x;
        for _ in 0..24 {
            let v = bez(t, x1, x2);
            if (v - x).abs() < 1e-5 {
                break;
            }
            if v < x {
                lo = t;
            } else {
                hi = t;
            }
            t = 0.5 * (lo + hi);
        }
        bez(t, y1, y2)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BoneKey {
    pub frame: u32,
    pub pos: Vec3,
    pub rot: Quat,
    /// X, Y, Z translation and rotation curves.
    pub curves: [Curve; 4],
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MorphKey {
    pub frame: u32,
    pub weight: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CameraKey {
    pub frame: u32,
    pub target: Vec3,
    /// Euler angles in radians (x pitch, y yaw, z roll).
    pub rot: Vec3,
    /// Negative distance means the camera sits in front of the target (MMD convention).
    pub distance: f32,
    pub fov: f32,
    pub ortho: bool,
    /// X, Y, Z, rotation, distance, field-of-view curves.
    pub curves: [Curve; 6],
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LightKey {
    pub frame: u32,
    pub color: Vec3,
    pub direction: Vec3,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ShadowKey {
    pub frame: u32,
    pub mode: u8,
    pub distance: f32,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ShowIkKey {
    pub frame: u32,
    pub visible: bool,
    pub ik: Vec<(String, bool)>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Motion {
    pub model_name: String,
    pub bones: BTreeMap<String, Vec<BoneKey>>,
    pub morphs: BTreeMap<String, Vec<MorphKey>>,
    pub camera: Vec<CameraKey>,
    pub light: Vec<LightKey>,
    pub shadow: Vec<ShadowKey>,
    pub show_ik: Vec<ShowIkKey>,
}

fn find_span<T>(keys: &[T], frame: f32, f: impl Fn(&T) -> u32) -> (usize, usize, f32) {
    if keys.len() == 1 || frame <= f(&keys[0]) as f32 {
        return (0, 0, 0.0);
    }
    let last = keys.len() - 1;
    if frame >= f(&keys[last]) as f32 {
        return (last, last, 0.0);
    }
    let i = keys.partition_point(|k| (f(k) as f32) <= frame);
    let a = i - 1;
    let fa = f(&keys[a]) as f32;
    let fb = f(&keys[i]) as f32;
    (a, i, ((frame - fa) / (fb - fa).max(1e-6)).clamp(0.0, 1.0))
}

pub fn sort_keys<T>(v: &mut Vec<T>, f: impl Fn(&T) -> u32) {
    v.sort_by_key(|k| f(k));
    v.dedup_by_key(|k| f(k));
}

impl Motion {
    /// Last keyed frame. Tracks are kept sorted, so only each track's last key is looked at
    /// (dense captured motions have thousands of keys per bone).
    pub fn max_frame(&self) -> u32 {
        let mut m = 0;
        for v in self.bones.values() {
            if let Some(k) = v.last() {
                m = m.max(k.frame);
            }
        }
        for v in self.morphs.values() {
            if let Some(k) = v.last() {
                m = m.max(k.frame);
            }
        }
        if let Some(k) = self.camera.last() {
            m = m.max(k.frame);
        }
        if let Some(k) = self.light.last() {
            m = m.max(k.frame);
        }
        m
    }

    /// Is there a bone key exactly at `frame`? (binary search)
    pub fn bone_key_at(&self, name: &str, frame: u32) -> bool {
        self.bones.get(name).map(|v| v.binary_search_by_key(&frame, |k| k.frame).is_ok()).unwrap_or(false)
    }

    pub fn morph_key_at(&self, name: &str, frame: u32) -> bool {
        self.morphs.get(name).map(|v| v.binary_search_by_key(&frame, |k| k.frame).is_ok()).unwrap_or(false)
    }

    /// Key frames of one track inside [from, to] (binary search, no full scan).
    pub fn track_frames_in(&self, track: &TrackId, from: u32, to: u32) -> Vec<u32> {
        fn slice<T>(v: &[T], f: impl Fn(&T) -> u32, a: u32, b: u32) -> Vec<u32> {
            let s = v.partition_point(|k| f(k) < a);
            let e = v.partition_point(|k| f(k) <= b);
            v[s..e.max(s)].iter().map(|k| f(k)).collect()
        }
        match track {
            TrackId::Bone(n) => self.bones.get(n).map(|v| slice(v, |k| k.frame, from, to)).unwrap_or_default(),
            TrackId::Morph(n) => self.morphs.get(n).map(|v| slice(v, |k| k.frame, from, to)).unwrap_or_default(),
            TrackId::Camera => slice(&self.camera, |k| k.frame, from, to),
            TrackId::Light => slice(&self.light, |k| k.frame, from, to),
        }
    }

    pub fn key_count(&self) -> usize {
        self.bones.values().map(|v| v.len()).sum::<usize>()
            + self.morphs.values().map(|v| v.len()).sum::<usize>()
            + self.camera.len()
            + self.light.len()
    }

    pub fn eval_bone(&self, name: &str, frame: f32) -> Option<(Vec3, Quat)> {
        let keys = self.bones.get(name)?;
        if keys.is_empty() {
            return None;
        }
        let (a, b, t) = find_span(keys, frame, |k| k.frame);
        let ka = &keys[a];
        if a == b {
            return Some((ka.pos, ka.rot));
        }
        let kb = &keys[b];
        let c = &kb.curves;
        let pos = Vec3::new(
            ka.pos.x + (kb.pos.x - ka.pos.x) * c[0].eval(t),
            ka.pos.y + (kb.pos.y - ka.pos.y) * c[1].eval(t),
            ka.pos.z + (kb.pos.z - ka.pos.z) * c[2].eval(t),
        );
        let rot = ka.rot.slerp(kb.rot, c[3].eval(t));
        Some((pos, rot))
    }

    pub fn eval_morph(&self, name: &str, frame: f32) -> Option<f32> {
        let keys = self.morphs.get(name)?;
        if keys.is_empty() {
            return None;
        }
        let (a, b, t) = find_span(keys, frame, |k| k.frame);
        Some(keys[a].weight + (keys[b].weight - keys[a].weight) * t)
    }

    pub fn eval_camera(&self, frame: f32) -> Option<CameraKey> {
        if self.camera.is_empty() {
            return None;
        }
        let (a, b, t) = find_span(&self.camera, frame, |k| k.frame);
        let ka = &self.camera[a];
        if a == b {
            return Some(ka.clone());
        }
        let kb = &self.camera[b];
        // MMD treats consecutive camera keys as a cut: no blending.
        if kb.frame == ka.frame + 1 {
            return Some(ka.clone());
        }
        let c = &kb.curves;
        let l = |x: f32, y: f32, cv: &Curve| x + (y - x) * cv.eval(t);
        Some(CameraKey {
            frame: frame as u32,
            target: Vec3::new(
                l(ka.target.x, kb.target.x, &c[0]),
                l(ka.target.y, kb.target.y, &c[1]),
                l(ka.target.z, kb.target.z, &c[2]),
            ),
            rot: ka.rot + (kb.rot - ka.rot) * c[3].eval(t),
            distance: l(ka.distance, kb.distance, &c[4]),
            fov: l(ka.fov, kb.fov, &c[5]),
            ortho: ka.ortho,
            curves: kb.curves,
        })
    }

    pub fn eval_light(&self, frame: f32) -> Option<(Vec3, Vec3)> {
        if self.light.is_empty() {
            return None;
        }
        let (a, b, t) = find_span(&self.light, frame, |k| k.frame);
        let ka = &self.light[a];
        let kb = &self.light[b];
        Some((ka.color.lerp(kb.color, t), ka.direction.lerp(kb.direction, t)))
    }

    pub fn eval_visible(&self, frame: f32) -> (bool, Vec<(String, bool)>) {
        if self.show_ik.is_empty() {
            return (true, vec![]);
        }
        let i = self.show_ik.partition_point(|k| k.frame as f32 <= frame);
        let k = &self.show_ik[i.saturating_sub(1)];
        (k.visible, k.ik.clone())
    }

    pub fn set_bone_key(&mut self, name: &str, key: BoneKey) {
        let v = self.bones.entry(name.to_string()).or_default();
        match v.binary_search_by_key(&key.frame, |k| k.frame) {
            Ok(i) => v[i] = key,
            Err(i) => v.insert(i, key),
        }
    }

    pub fn set_morph_key(&mut self, name: &str, key: MorphKey) {
        let v = self.morphs.entry(name.to_string()).or_default();
        match v.binary_search_by_key(&key.frame, |k| k.frame) {
            Ok(i) => v[i] = key,
            Err(i) => v.insert(i, key),
        }
    }

    pub fn set_camera_key(&mut self, key: CameraKey) {
        match self.camera.binary_search_by_key(&key.frame, |k| k.frame) {
            Ok(i) => self.camera[i] = key,
            Err(i) => self.camera.insert(i, key),
        }
    }

    pub fn set_light_key(&mut self, key: LightKey) {
        match self.light.binary_search_by_key(&key.frame, |k| k.frame) {
            Ok(i) => self.light[i] = key,
            Err(i) => self.light.insert(i, key),
        }
    }

    /// Remove keys on one track at a frame. Returns true when something was removed.
    pub fn delete_key(&mut self, track: &TrackId, frame: u32) -> bool {
        match track {
            TrackId::Bone(n) => {
                if let Some(v) = self.bones.get_mut(n) {
                    let l = v.len();
                    v.retain(|k| k.frame != frame);
                    return v.len() != l;
                }
            }
            TrackId::Morph(n) => {
                if let Some(v) = self.morphs.get_mut(n) {
                    let l = v.len();
                    v.retain(|k| k.frame != frame);
                    return v.len() != l;
                }
            }
            TrackId::Camera => {
                let l = self.camera.len();
                self.camera.retain(|k| k.frame != frame);
                return l != self.camera.len();
            }
            TrackId::Light => {
                let l = self.light.len();
                self.light.retain(|k| k.frame != frame);
                return l != self.light.len();
            }
        }
        false
    }

    /// Move a key from one frame to another (overwrites what is there).
    pub fn move_key(&mut self, track: &TrackId, from: u32, to: u32) {
        if from == to {
            return;
        }
        match track {
            TrackId::Bone(n) => {
                if let Some(v) = self.bones.get_mut(n) {
                    if let Some(i) = v.iter().position(|k| k.frame == from) {
                        let mut k = v.remove(i);
                        k.frame = to;
                        v.retain(|k| k.frame != to);
                        let p = v.partition_point(|x| x.frame < to);
                        v.insert(p, k);
                    }
                }
            }
            TrackId::Morph(n) => {
                if let Some(v) = self.morphs.get_mut(n) {
                    if let Some(i) = v.iter().position(|k| k.frame == from) {
                        let mut k = v.remove(i);
                        k.frame = to;
                        v.retain(|k| k.frame != to);
                        let p = v.partition_point(|x| x.frame < to);
                        v.insert(p, k);
                    }
                }
            }
            TrackId::Camera => {
                if let Some(i) = self.camera.iter().position(|k| k.frame == from) {
                    let mut k = self.camera.remove(i);
                    k.frame = to;
                    self.camera.retain(|k| k.frame != to);
                    let p = self.camera.partition_point(|x| x.frame < to);
                    self.camera.insert(p, k);
                }
            }
            TrackId::Light => {
                if let Some(i) = self.light.iter().position(|k| k.frame == from) {
                    let mut k = self.light.remove(i);
                    k.frame = to;
                    self.light.retain(|k| k.frame != to);
                    let p = self.light.partition_point(|x| x.frame < to);
                    self.light.insert(p, k);
                }
            }
        }
    }

    pub fn track_frames(&self, track: &TrackId) -> Vec<u32> {
        match track {
            TrackId::Bone(n) => self.bones.get(n).map(|v| v.iter().map(|k| k.frame).collect()).unwrap_or_default(),
            TrackId::Morph(n) => self.morphs.get(n).map(|v| v.iter().map(|k| k.frame).collect()).unwrap_or_default(),
            TrackId::Camera => self.camera.iter().map(|k| k.frame).collect(),
            TrackId::Light => self.light.iter().map(|k| k.frame).collect(),
        }
    }

    /// Merge another motion in, offset by `at` frames.
    pub fn merge(&mut self, other: &Motion, at: u32) {
        for (n, keys) in &other.bones {
            for k in keys {
                let mut k = k.clone();
                k.frame += at;
                self.set_bone_key(n, k);
            }
        }
        for (n, keys) in &other.morphs {
            for k in keys {
                let mut k = k.clone();
                k.frame += at;
                self.set_morph_key(n, k);
            }
        }
        for k in &other.camera {
            let mut k = k.clone();
            k.frame += at;
            self.set_camera_key(k);
        }
        for k in &other.light {
            let mut k = k.clone();
            k.frame += at;
            self.set_light_key(k);
        }
        self.shadow.extend(other.shadow.iter().cloned());
        self.show_ik.extend(other.show_ik.iter().cloned());
        sort_keys(&mut self.shadow, |k| k.frame);
        sort_keys(&mut self.show_ik, |k| k.frame);
    }

    /// VMD files keep only the first 15 bytes of each bone and morph name, so long names arrive
    /// cut short. Give such tracks the model's full names. Returns how many were renamed.
    pub fn fit_to_model(&mut self, data: &crate::model::ModelData) -> usize {
        use crate::formats::reader::fit_name;
        let mut renamed = 0;
        let bone_map: std::collections::HashMap<String, String> = data
            .bones
            .iter()
            .filter_map(|b| {
                let f = fit_name(&b.name, 15);
                (f != b.name).then(|| (f, b.name.clone()))
            })
            .collect();
        let names: Vec<String> = self.bones.keys().filter(|n| data.bone_index(n).is_none() && bone_map.contains_key(*n)).cloned().collect();
        for n in names {
            if let Some(keys) = self.bones.remove(&n) {
                self.bones.entry(bone_map[&n].clone()).or_default().extend(keys);
                renamed += 1;
            }
        }
        let morph_map: std::collections::HashMap<String, String> = data
            .morphs
            .iter()
            .filter_map(|m| {
                let f = fit_name(&m.name, 15);
                (f != m.name).then(|| (f, m.name.clone()))
            })
            .collect();
        let names: Vec<String> = self.morphs.keys().filter(|n| data.morph_index(n).is_none() && morph_map.contains_key(*n)).cloned().collect();
        for n in names {
            if let Some(keys) = self.morphs.remove(&n) {
                self.morphs.entry(morph_map[&n].clone()).or_default().extend(keys);
                renamed += 1;
            }
        }
        if renamed > 0 {
            for v in self.bones.values_mut() {
                sort_keys(v, |k| k.frame);
            }
            for v in self.morphs.values_mut() {
                sort_keys(v, |k| k.frame);
            }
        }
        renamed
    }

    /// Merge into this motion. Tracks this motion doesn't have yet are moved in whole, which keeps
    /// loading big captured dances (hundreds of thousands of keys) fast.
    pub fn merge_owned(&mut self, mut other: Motion, at: u32) {
        let bones = std::mem::take(&mut other.bones);
        for (n, mut keys) in bones {
            if !self.bones.contains_key(&n) {
                for k in &mut keys {
                    k.frame += at;
                }
                self.bones.insert(n, keys);
            } else {
                for mut k in keys {
                    k.frame += at;
                    self.set_bone_key(&n, k);
                }
            }
        }
        let morphs = std::mem::take(&mut other.morphs);
        for (n, mut keys) in morphs {
            if !self.morphs.contains_key(&n) {
                for k in &mut keys {
                    k.frame += at;
                }
                self.morphs.insert(n, keys);
            } else {
                for mut k in keys {
                    k.frame += at;
                    self.set_morph_key(&n, k);
                }
            }
        }
        self.merge(&other, at);
    }

    pub fn is_camera_motion(&self) -> bool {
        self.bones.is_empty() && self.morphs.is_empty() && (!self.camera.is_empty() || !self.light.is_empty())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TrackId {
    Bone(String),
    Morph(String),
    Camera,
    Light,
}

pub fn euler_deg(q: Quat) -> Vec3 {
    let (y, x, z) = q.to_euler(EulerRot::YXZ);
    Vec3::new(x.to_degrees(), y.to_degrees(), z.to_degrees())
}

pub fn quat_from_euler_deg(e: Vec3) -> Quat {
    Quat::from_euler(EulerRot::YXZ, e.y.to_radians(), e.x.to_radians(), e.z.to_radians())
}

/// The file format of this app's own motions (.hmo): readable JSON.
#[derive(Serialize, Deserialize)]
pub struct OwnMotionFile {
    pub format: String,
    pub version: u32,
    pub fps: f32,
    pub motion: Motion,
}

pub fn save_own(m: &Motion, fps: f32) -> Result<String, String> {
    serde_json::to_string_pretty(&OwnMotionFile { format: "hanabi-motion".into(), version: 1, fps, motion: m.clone() })
        .map_err(|e| e.to_string())
}

pub fn load_own(s: &str) -> Result<Motion, String> {
    let f: OwnMotionFile = serde_json::from_str(s).map_err(|e| e.to_string())?;
    if f.format != "hanabi-motion" {
        return Err("not a Hanabi motion file".into());
    }
    Ok(f.motion)
}
