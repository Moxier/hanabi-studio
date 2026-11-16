//! Easy animation helpers: mirroring, pose clipboard and procedural motion generators.

use crate::model::{mirror_name, ModelData};
use crate::motion::*;
use glam::{Quat, Vec3};

/// Find the first bone that exists among several common names.
pub fn find_bone(m: &ModelData, names: &[&str]) -> Option<String> {
    for n in names {
        if m.bone_index(n).is_some() {
            return Some(n.to_string());
        }
    }
    // case-insensitive fallback for English names
    for n in names {
        if let Some(b) = m.bones.iter().find(|b| b.name.eq_ignore_ascii_case(n) || b.name_en.eq_ignore_ascii_case(n)) {
            return Some(b.name.clone());
        }
    }
    None
}

pub const CENTER: &[&str] = &["センター", "center", "Center"];
pub const UPPER: &[&str] = &["上半身", "upper body", "UpperBody"];
pub const UPPER2: &[&str] = &["上半身2", "upper body 2", "UpperBody2"];
pub const NECK: &[&str] = &["首", "neck", "Neck"];
pub const HEAD: &[&str] = &["頭", "head", "Head"];
pub const ARM_R: &[&str] = &["右腕", "arm_R", "Arm_R", "RightArm"];
pub const ELBOW_R: &[&str] = &["右ひじ", "elbow_R", "Elbow_R", "RightElbow"];
pub const WRIST_R: &[&str] = &["右手首", "wrist_R", "Wrist_R", "RightWrist"];
pub const ARM_L: &[&str] = &["左腕", "arm_L", "Arm_L", "LeftArm"];
pub const ELBOW_L: &[&str] = &["左ひじ", "elbow_L", "Elbow_L", "LeftElbow"];

/// Mirror a rotation/translation across the model's left-right (X) plane.
pub fn mirror_rt(p: Vec3, r: Quat) -> (Vec3, Quat) {
    (Vec3::new(-p.x, p.y, p.z), Quat::from_xyzw(r.x, -r.y, -r.z, r.w))
}

#[derive(Clone, Debug, Default)]
pub struct PoseClip {
    pub bones: Vec<(String, Vec3, Quat)>,
}

pub fn mirror_pose(clip: &PoseClip) -> PoseClip {
    PoseClip {
        bones: clip
            .bones
            .iter()
            .map(|(n, p, r)| {
                let (mp, mr) = mirror_rt(*p, *r);
                (mirror_name(n).unwrap_or_else(|| n.clone()), mp, mr)
            })
            .collect(),
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Generator {
    Idle,
    Wave,
    Nod,
    Bounce,
}

impl Generator {
    pub const ALL: [Generator; 4] = [Generator::Idle, Generator::Wave, Generator::Nod, Generator::Bounce];
    pub fn label(self) -> &'static str {
        match self {
            Generator::Idle => "Idle breathing",
            Generator::Wave => "Hand wave",
            Generator::Nod => "Head nod",
            Generator::Bounce => "Bounce to the beat",
        }
    }
    pub fn from_name(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "idle" => Some(Generator::Idle),
            "wave" => Some(Generator::Wave),
            "nod" => Some(Generator::Nod),
            "bounce" => Some(Generator::Bounce),
            _ => None,
        }
    }
}

/// Write procedural keys into `motion`. Returns how many keys were created.
pub fn generate(g: Generator, m: &ModelData, motion: &mut Motion, start: u32, length: u32, bpm: f32, fps: f32) -> usize {
    let deg = |x: f32, y: f32, z: f32| quat_from_euler_deg(Vec3::new(x, y, z));
    let mut n = 0;
    let mut put = |motion: &mut Motion, names: &[&str], f: u32, pos: Vec3, rot: Quat| {
        if let Some(b) = find_bone(m, names) {
            motion.set_bone_key(&b, BoneKey { frame: f, pos, rot, curves: [Curve::EASE_IN_OUT; 4] });
            n += 1;
        }
    };
    let len = length.max(10);
    match g {
        Generator::Idle => {
            let step = 15;
            let mut f = 0;
            while f <= len {
                let t = f as f32 / fps;
                let br = (t * std::f32::consts::TAU / 3.5).sin();
                let sw = (t * std::f32::consts::TAU / 7.0).sin();
                put(motion, UPPER, start + f, Vec3::ZERO, deg(br * 1.5, sw * 2.0, 0.0));
                put(motion, UPPER2, start + f, Vec3::ZERO, deg(br * 1.0, 0.0, 0.0));
                put(motion, HEAD, start + f, Vec3::ZERO, deg(-br * 1.0, sw * 3.0, sw * 1.5));
                put(motion, CENTER, start + f, Vec3::new(sw * 0.15, br * 0.05, 0.0), Quat::IDENTITY);
                f += step;
            }
        }
        Generator::Wave => {
            let mut f = 0;
            let mut i = 0;
            put(motion, ARM_R, start, Vec3::ZERO, Quat::IDENTITY);
            while f <= len {
                let side = if i % 2 == 0 { 1.0 } else { -1.0 };
                if f > 0 {
                    put(motion, ARM_R, start + f, Vec3::ZERO, deg(0.0, -20.0, -70.0));
                    put(motion, ELBOW_R, start + f, Vec3::ZERO, deg(0.0, -35.0 + side * 25.0, 0.0));
                    put(motion, WRIST_R, start + f, Vec3::ZERO, deg(0.0, 0.0, side * 15.0));
                }
                f += 8;
                i += 1;
            }
        }
        Generator::Nod => {
            let mut f = 0;
            let mut i = 0;
            while f <= len {
                let a = if i % 2 == 0 { 0.0 } else { 14.0 };
                put(motion, HEAD, start + f, Vec3::ZERO, deg(a, 0.0, 0.0));
                put(motion, NECK, start + f, Vec3::ZERO, deg(a * 0.4, 0.0, 0.0));
                f += 10;
                i += 1;
            }
        }
        Generator::Bounce => {
            let beat = (60.0 / bpm.max(30.0) * fps).max(4.0);
            let mut k = 0.0f32;
            while (k * beat) as u32 <= len {
                let f = start + (k * beat) as u32;
                let half = start + ((k + 0.5) * beat) as u32;
                let sway = if (k as i32) % 2 == 0 { 1.0 } else { -1.0 };
                put(motion, CENTER, f, Vec3::new(0.0, -0.8, 0.0), deg(0.0, sway * 6.0, 0.0));
                put(motion, CENTER, half, Vec3::new(sway * 0.3, 0.0, 0.0), deg(0.0, sway * 3.0, 0.0));
                put(motion, UPPER, f, Vec3::ZERO, deg(6.0, -sway * 4.0, sway * 3.0));
                put(motion, UPPER, half, Vec3::ZERO, deg(-2.0, 0.0, -sway * 2.0));
                put(motion, HEAD, f, Vec3::ZERO, deg(-5.0, sway * 6.0, 0.0));
                put(motion, ARM_L, f, Vec3::ZERO, deg(0.0, 0.0, -40.0 + sway * 10.0));
                put(motion, ARM_R, f, Vec3::ZERO, deg(0.0, 0.0, 40.0 + sway * 10.0));
                put(motion, ELBOW_L, f, Vec3::ZERO, deg(0.0, -30.0, 0.0));
                put(motion, ELBOW_R, f, Vec3::ZERO, deg(0.0, 30.0, 0.0));
                k += 1.0;
            }
        }
    }
    n
}

/// Average each selected rotation key with its neighbours (reduces jitter from captured motion).
pub fn smooth_track(keys: &mut [BoneKey], strength: f32) {
    if keys.len() < 3 {
        return;
    }
    let orig: Vec<(Vec3, Quat)> = keys.iter().map(|k| (k.pos, k.rot)).collect();
    for i in 1..keys.len() - 1 {
        let (pp, pr) = orig[i - 1];
        let (np, nr) = orig[i + 1];
        let mid_p = (pp + np) * 0.5;
        let mid_r = pr.slerp(nr, 0.5);
        keys[i].pos = orig[i].0.lerp(mid_p, strength * 0.5);
        keys[i].rot = orig[i].1.slerp(mid_r, strength * 0.5);
    }
}
