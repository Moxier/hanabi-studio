//! Bone hierarchy evaluation: keyframe pose, inherited ("append") rotation/translation, and CCD IK.

use crate::model::{Bone, ModelData};
use glam::{EulerRot, Mat4, Quat, Vec3};

#[derive(Clone, Copy, Debug)]
pub struct BoneState {
    pub anim_pos: Vec3,
    pub anim_rot: Quat,
    pub morph_pos: Vec3,
    pub morph_rot: Quat,
    pub ik_rot: Quat,
    pub append_pos: Vec3,
    pub append_rot: Quat,
    pub local_rot: Quat,
    pub global: Mat4,
    /// Set when physics wrote the global transform this frame.
    pub physics_driven: bool,
}

impl Default for BoneState {
    fn default() -> Self {
        Self {
            anim_pos: Vec3::ZERO,
            anim_rot: Quat::IDENTITY,
            morph_pos: Vec3::ZERO,
            morph_rot: Quat::IDENTITY,
            ik_rot: Quat::IDENTITY,
            append_pos: Vec3::ZERO,
            append_rot: Quat::IDENTITY,
            local_rot: Quat::IDENTITY,
            global: Mat4::IDENTITY,
            physics_driven: false,
        }
    }
}

pub struct Skeleton {
    pub states: Vec<BoneState>,
    pub order: Vec<usize>,
    pub children: Vec<Vec<usize>>,
    pub ik_enabled: Vec<bool>,
    pub ik_globally_enabled: bool,
}

impl Skeleton {
    pub fn new(m: &ModelData) -> Self {
        let n = m.bones.len();
        let mut order: Vec<usize> = (0..n).collect();
        order.sort_by_key(|&i| (m.bones[i].after_physics, m.bones[i].level, i));
        let mut children = vec![Vec::new(); n];
        for (i, b) in m.bones.iter().enumerate() {
            if b.parent >= 0 && (b.parent as usize) < n && b.parent as usize != i {
                children[b.parent as usize].push(i);
            }
        }
        let mut s = Self {
            states: vec![BoneState::default(); n],
            order,
            children,
            ik_enabled: vec![true; n],
            ik_globally_enabled: true,
        };
        for i in 0..n {
            s.states[i].global = Mat4::from_translation(m.bones[i].position);
        }
        s
    }

    pub fn reset_anim(&mut self) {
        for s in &mut self.states {
            s.anim_pos = Vec3::ZERO;
            s.anim_rot = Quat::IDENTITY;
            s.morph_pos = Vec3::ZERO;
            s.morph_rot = Quat::IDENTITY;
            s.physics_driven = false;
        }
    }

    fn parent_global(&self, bones: &[Bone], i: usize) -> Mat4 {
        let p = bones[i].parent;
        if p >= 0 && (p as usize) < bones.len() && p as usize != i {
            self.states[p as usize].global
        } else {
            Mat4::IDENTITY
        }
    }

    fn rest_offset(bones: &[Bone], i: usize) -> Vec3 {
        let p = bones[i].parent;
        if p >= 0 && (p as usize) < bones.len() {
            bones[i].position - bones[p as usize].position
        } else {
            bones[i].position
        }
    }

    /// Rotation before inheritance: keyframe * morph, then IK correction.
    fn base_rot(&self, i: usize) -> Quat {
        let s = &self.states[i];
        (s.morph_rot * s.anim_rot) * s.ik_rot
    }

    fn compute_local(&mut self, bones: &[Bone], i: usize) -> (Vec3, Quat) {
        let b = &bones[i];
        let mut rot = self.base_rot(i);
        let mut pos = self.states[i].anim_pos + self.states[i].morph_pos;
        if b.append_parent >= 0 && (b.append_parent as usize) < bones.len() && b.append_parent as usize != i {
            let ap = b.append_parent as usize;
            let ab = &bones[ap];
            if b.append_rotate {
                let src = if ab.append_parent >= 0 && ab.append_rotate {
                    self.states[ap].append_rot * self.base_rot(ap)
                } else {
                    self.base_rot(ap)
                };
                let r = if b.append_local { self.states[ap].local_rot } else { src };
                let a = Quat::IDENTITY.slerp(r, b.append_ratio);
                self.states[i].append_rot = a;
                rot = a * rot;
            }
            if b.append_translate {
                let src = if ab.append_parent >= 0 && ab.append_translate {
                    self.states[ap].append_pos
                } else {
                    self.states[ap].anim_pos + self.states[ap].morph_pos
                };
                let t = src * b.append_ratio;
                self.states[i].append_pos = t;
                pos += t;
            }
        }
        if let Some(axis) = b.fixed_axis {
            // Keep only the twist around the fixed axis.
            let v = Vec3::new(rot.x, rot.y, rot.z);
            let proj = axis * v.dot(axis);
            let q = Quat::from_xyzw(proj.x, proj.y, proj.z, rot.w);
            if q.length_squared() > 1e-8 {
                rot = q.normalize();
            }
        }
        (pos + Self::rest_offset(bones, i), rot)
    }

    fn update_bone(&mut self, bones: &[Bone], i: usize) {
        let (t, r) = self.compute_local(bones, i);
        self.states[i].local_rot = r;
        self.states[i].global = self.parent_global(bones, i) * Mat4::from_rotation_translation(r, t);
    }

    fn update_subtree(&mut self, bones: &[Bone], i: usize) {
        self.update_bone(bones, i);
        let kids = self.children[i].clone();
        for c in kids {
            if !self.states[c].physics_driven {
                self.update_subtree(bones, c);
            }
        }
    }

    pub fn global_pos(&self, i: usize) -> Vec3 {
        self.states[i].global.w_axis.truncate()
    }

    /// Evaluate bones for one phase (before or after physics).
    pub fn update(&mut self, m: &ModelData, after_physics: bool) {
        let bones = &m.bones;
        let order = self.order.clone();
        for &i in &order {
            if bones[i].after_physics != after_physics {
                continue;
            }
            if self.states[i].physics_driven {
                continue;
            }
            self.update_bone(bones, i);
            if bones[i].ik.is_some() && self.ik_enabled[i] && self.ik_globally_enabled {
                self.solve_ik(bones, i);
            }
        }
    }

    /// After physics wrote bone transforms, refresh children that follow them.
    pub fn refresh_after_physics(&mut self, m: &ModelData) {
        let bones = &m.bones;
        let order = self.order.clone();
        for &i in &order {
            if self.states[i].physics_driven || bones[i].after_physics {
                continue;
            }
            let p = bones[i].parent;
            if p >= 0 && (p as usize) < bones.len() {
                let (t, r) = self.compute_local(bones, i);
                self.states[i].global = self.states[p as usize].global * Mat4::from_rotation_translation(r, t);
            }
        }
    }

    pub fn clear_ik(&mut self) {
        for s in &mut self.states {
            s.ik_rot = Quat::IDENTITY;
        }
    }

    fn solve_ik(&mut self, bones: &[Bone], ik_i: usize) {
        let ik = match &bones[ik_i].ik {
            Some(ik) => ik.clone(),
            None => return,
        };
        if ik.target < 0 || ik.target as usize >= bones.len() || ik.links.is_empty() {
            return;
        }
        let target = ik.target as usize;
        for l in &ik.links {
            if l.bone >= 0 && (l.bone as usize) < bones.len() {
                self.states[l.bone as usize].ik_rot = Quat::IDENTITY;
            }
        }
        // Refresh the chain from the link closest to the root.
        if let Some(root) = ik.links.last() {
            if root.bone >= 0 && (root.bone as usize) < bones.len() {
                self.update_subtree(bones, root.bone as usize);
            }
        }
        let goal = self.global_pos(ik_i);
        let iterations = ik.iterations.clamp(1, 256);
        let limit = if ik.limit_angle > 0.0 { ik.limit_angle } else { 1.0 };
        for it in 0..iterations {
            for link in &ik.links {
                let lb = link.bone;
                if lb < 0 || lb as usize >= bones.len() || lb as usize == target {
                    continue;
                }
                let lb = lb as usize;
                let inv = self.states[lb].global.inverse();
                let tp = self.global_pos(target);
                let lt = inv.transform_point3(tp).normalize_or_zero();
                let lg = inv.transform_point3(goal).normalize_or_zero();
                if lt == Vec3::ZERO || lg == Vec3::ZERO {
                    continue;
                }
                let dot = lt.dot(lg).clamp(-1.0, 1.0);
                let mut angle = dot.acos();
                if angle < 1e-5 {
                    continue;
                }
                angle = angle.min(limit);
                let axis = lt.cross(lg);
                if axis.length_squared() < 1e-12 {
                    continue;
                }
                let delta = Quat::from_axis_angle(axis.normalize(), angle);
                let pre = self.states[lb].morph_rot * self.states[lb].anim_rot;
                let mut total = (pre * self.states[lb].ik_rot * delta).normalize();
                if let Some((mn, mx)) = link.limit {
                    let x_only = mn.y.abs() < 1e-6 && mx.y.abs() < 1e-6 && mn.z.abs() < 1e-6 && mx.z.abs() < 1e-6;
                    if x_only {
                        // Hinge (e.g. knees): keep rotation about X only.
                        let (mut x, _, _) = total.to_euler(EulerRot::XYZ);
                        if it == 0 && x > mx.x && -x >= mn.x {
                            x = -x;
                        }
                        let x = x.clamp(mn.x, mx.x);
                        total = Quat::from_rotation_x(x);
                    } else {
                        let (x, y, z) = total.to_euler(EulerRot::XYZ);
                        total = Quat::from_euler(
                            EulerRot::XYZ,
                            x.clamp(mn.x, mx.x),
                            y.clamp(mn.y, mx.y),
                            z.clamp(mn.z, mx.z),
                        );
                    }
                }
                self.states[lb].ik_rot = pre.inverse() * total;
                self.update_subtree(bones, lb);
            }
            if (self.global_pos(target) - goal).length_squared() < 1e-8 {
                break;
            }
        }
    }

    pub fn skin_matrices(&self, m: &ModelData, out: &mut Vec<Mat4>) {
        out.clear();
        for (i, s) in self.states.iter().enumerate() {
            out.push(s.global * Mat4::from_translation(-m.bones[i].position));
        }
    }
}
