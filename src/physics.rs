//! Rigid body + joint simulation for hair, skirts and accessories, built on the Rapier engine.

use crate::model::{ModelData, RigidMode, Shape};
use crate::skeleton::Skeleton;
use glam::{EulerRot, Mat4, Quat, Vec3};
use rapier3d::prelude::*;
use serde::{Deserialize, Serialize};

/// What to do when a character jumps far between two frames.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BigMove {
    /// Put hair and cloth straight back on their bones.
    Snap,
    /// Carry them along with the character, keeping some swing.
    Soften,
    /// Leave the simulation alone.
    Off,
}

/// Scene-wide physics settings (saved with the project).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PhysicsCfg {
    pub gravity: f32,
    pub gravity_dir: [f32; 3],
    pub wind_strength: f32,
    /// Wind direction in degrees around the vertical axis (0 = toward +Z).
    pub wind_dir: f32,
    pub wind_gusts: f32,
    pub air_drag: f32,
    pub max_speed: f32,
    pub big_move: BigMove,
    /// Distance (in model units per frame) that counts as a big move.
    pub big_move_dist: f32,
    pub settle_steps: u32,
}

impl Default for PhysicsCfg {
    fn default() -> Self {
        Self {
            gravity: 98.0,
            gravity_dir: [0.0, -1.0, 0.0],
            wind_strength: 0.0,
            wind_dir: 0.0,
            wind_gusts: 0.4,
            air_drag: 0.0,
            max_speed: 250.0,
            big_move: BigMove::Snap,
            big_move_dist: 4.0,
            settle_steps: 40,
        }
    }
}

impl PhysicsCfg {
    pub fn gravity_vec(&self) -> Vec3 {
        Vec3::from(self.gravity_dir).normalize_or(Vec3::NEG_Y) * self.gravity
    }
    pub fn wind_vec(&self) -> Vec3 {
        let a = self.wind_dir.to_radians();
        Vec3::new(a.sin(), 0.0, a.cos()) * self.wind_strength
    }
}

pub struct Physics {
    pipeline: PhysicsPipeline,
    params: IntegrationParameters,
    islands: IslandManager,
    broad: BroadPhaseBvh,
    narrow: NarrowPhase,
    bodies: RigidBodySet,
    colliders: ColliderSet,
    joints: ImpulseJointSet,
    multibody: MultibodyJointSet,
    ccd: CCDSolver,
    handles: Vec<Option<RigidBodyHandle>>,
    /// Rigid body transform relative to its bone.
    offsets: Vec<Mat4>,
    inv_offsets: Vec<Mat4>,
    modes: Vec<RigidMode>,
    bones: Vec<i32>,
    base_damping: Vec<f32>,
    pub gravity: Vec3,
    pub substeps: u32,
    /// Times the simulation was snapped back after a big move (shown in the Physics window).
    pub snaps: u32,
    /// Set when the physics engine failed on this model; physics then stays off for it
    /// instead of taking the whole app down.
    pub failed: bool,
    /// Joints that hold two pieces together, ordered outward from the parts that follow bones:
    /// (parent body, child body, anchor in parent space, anchor in child space).
    chain: Vec<(usize, usize, Vec3, Vec3)>,
    /// Parent bone of each body's bone, and the body indices in parent-first bone order.
    bone_parent: Vec<i32>,
    order: Vec<usize>,
}

fn to_pose(m: &Mat4) -> Pose {
    let (_, r, t) = m.to_scale_rotation_translation();
    if !t.is_finite() || !r.is_finite() || r.length_squared() < 1e-12 {
        return Pose::from_parts(if t.is_finite() { t } else { Vec3::ZERO }, Quat::IDENTITY);
    }
    Pose::from_parts(t, r.normalize())
}

fn from_pose(p: &Pose) -> Mat4 {
    Mat4::from_rotation_translation(p.rotation, p.translation)
}

fn damping(d: f32) -> f32 {
    -(1.0 - d.clamp(0.0, 0.999)).ln() * 1.0
}

impl Physics {
    pub fn new(m: &ModelData, skel: &Skeleton) -> Option<Self> {
        if m.rigid_bodies.is_empty() {
            return None;
        }
        let mut bodies = RigidBodySet::new();
        let mut colliders = ColliderSet::new();
        let mut handles = Vec::new();
        let mut offsets = Vec::new();
        let mut inv_offsets = Vec::new();
        let mut modes = Vec::new();
        let mut bone_ids = Vec::new();
        let mut base_damping = Vec::new();
        for rb in &m.rigid_bodies {
            let rot = Quat::from_euler(EulerRot::YXZ, rb.rotation.y, rb.rotation.x, rb.rotation.z);
            let world = Mat4::from_rotation_translation(rot, rb.position);
            let bone = if rb.bone >= 0 && (rb.bone as usize) < m.bones.len() { rb.bone } else { -1 };
            let bone_rest = if bone >= 0 { Mat4::from_translation(m.bones[bone as usize].position) } else { Mat4::IDENTITY };
            let offset = bone_rest.inverse() * world;
            let bone_global = if bone >= 0 { skel.states[bone as usize].global } else { Mat4::IDENTITY };
            let start = bone_global * offset;
            let builder = match rb.mode {
                RigidMode::FollowBone => RigidBodyBuilder::kinematic_position_based(),
                _ => RigidBodyBuilder::dynamic()
                    .linear_damping(damping(rb.linear_damping))
                    .angular_damping(damping(rb.angular_damping)),
            };
            let body = builder.pose(to_pose(&start)).build();
            let h = bodies.insert(body);
            let s = rb.size.max(Vec3::splat(0.01));
            let shape = match rb.shape {
                Shape::Sphere => ColliderBuilder::ball(s.x),
                Shape::Box => ColliderBuilder::cuboid(s.x, s.y, s.z),
                Shape::Capsule => ColliderBuilder::capsule_y(s.y * 0.5, s.x),
            };
            let groups = InteractionGroups::new(
                Group::from_bits_truncate(1u32 << (rb.group.min(15) as u32)),
                Group::from_bits_truncate(rb.mask as u32),
                InteractionTestMode::And,
            );
            let col = shape
                .mass(if rb.mode == RigidMode::FollowBone { 1.0 } else { rb.mass.max(0.001) })
                .friction(rb.friction)
                .restitution(rb.restitution)
                .collision_groups(groups)
                .build();
            colliders.insert_with_parent(col, h, &mut bodies);
            handles.push(Some(h));
            offsets.push(offset);
            inv_offsets.push(offset.inverse());
            modes.push(rb.mode);
            bone_ids.push(bone);
            base_damping.push(damping(rb.linear_damping));
        }
        let mut joints = ImpulseJointSet::new();
        let mut links: Vec<(usize, usize, Vec3, Vec3)> = Vec::new();

        for j in &m.joints {
            let (a, b) = (j.a, j.b);
            if a < 0 || b < 0 || a as usize >= handles.len() || b as usize >= handles.len() || a == b {
                continue;
            }
            let (ha, hb) = (handles[a as usize].unwrap(), handles[b as usize].unwrap());
            let rot = Quat::from_euler(EulerRot::YXZ, j.rotation.y, j.rotation.x, j.rotation.z);
            let jw = Mat4::from_rotation_translation(rot, j.position);
            let pa = from_pose(bodies[ha].position());
            let pb = from_pose(bodies[hb].position());
            let f1 = to_pose(&(pa.inverse() * jw));
            let f2 = to_pose(&(pb.inverse() * jw));
            let lin_locked = (0..3).all(|k| (j.lin_max[k] - j.lin_min[k]).abs() < 1e-3);
            // Only links that mirror the bone hierarchy (child bone hangs from the parent's
            // bone) are corrected; helper links between neighbours are left to the engine.
            let ba = m.rigid_bodies[a as usize].bone;
            let bb = m.rigid_bodies[b as usize].bone;
            let parent_of = |x: i32| if x >= 0 && (x as usize) < m.bones.len() { m.bones[x as usize].parent } else { -2 };
            let follows_bones = ba >= 0 && bb >= 0 && (parent_of(bb) == ba || parent_of(ba) == bb);
            if lin_locked && follows_bones {
                links.push((a as usize, b as usize, (pa.inverse() * jw).w_axis.truncate(), (pb.inverse() * jw).w_axis.truncate()));
            }
            let mut locked = JointAxesMask::empty();
            let lin = [JointAxis::LinX, JointAxis::LinY, JointAxis::LinZ];
            let ang = [JointAxis::AngX, JointAxis::AngY, JointAxis::AngZ];
            let lin_bits = [JointAxesMask::LIN_X, JointAxesMask::LIN_Y, JointAxesMask::LIN_Z];
            let ang_bits = [JointAxesMask::ANG_X, JointAxesMask::ANG_Y, JointAxesMask::ANG_Z];
            for k in 0..3 {
                if (j.lin_max[k] - j.lin_min[k]).abs() < 1e-5 {
                    locked |= lin_bits[k];
                }
                if (j.ang_max[k] - j.ang_min[k]).abs() < 1e-5 {
                    locked |= ang_bits[k];
                }
            }
            let mut gj = GenericJointBuilder::new(locked).local_frame1(f1).local_frame2(f2).contacts_enabled(false).build();
            for k in 0..3 {
                if !locked.contains(lin_bits[k]) && j.lin_min[k] < j.lin_max[k] {
                    gj.set_limits(lin[k], [j.lin_min[k], j.lin_max[k]]);
                }
                if !locked.contains(ang_bits[k]) && j.ang_min[k] < j.ang_max[k] {
                    let lo = j.ang_min[k].max(-std::f32::consts::PI);
                    let hi = j.ang_max[k].min(std::f32::consts::PI);
                    gj.set_limits(ang[k], [lo, hi]);
                }
                if j.spring_lin[k] > 0.0 && !locked.contains(lin_bits[k]) {
                    gj.set_motor(lin[k], 0.0, 0.0, j.spring_lin[k], 0.1);
                }
                if j.spring_ang[k] > 0.0 && !locked.contains(ang_bits[k]) {
                    gj.set_motor(ang[k], 0.0, 0.0, j.spring_ang[k], 0.1);
                }
            }
            joints.insert(ha, hb, gj, true);
        }
        // Order the links outward from the bodies that follow bones (breadth first), so each
        // piece is corrected after the piece it hangs from.
        let n = handles.len();
        let mut adj: Vec<Vec<(usize, Vec3, Vec3)>> = vec![Vec::new(); n];
        for &(a, b, la, lb) in &links {
            let ba = m.rigid_bodies[a].bone;
            let bb = m.rigid_bodies[b].bone;
            if m.bones[bb as usize].parent == ba {
                adj[a].push((b, la, lb));
            } else {
                adj[b].push((a, lb, la));
            }
        }
        let mut seen: Vec<bool> = modes.iter().map(|m| *m == RigidMode::FollowBone).collect();
        let mut queue: std::collections::VecDeque<usize> = (0..n).filter(|&i| seen[i]).collect();
        let mut chain = Vec::new();
        while let Some(p) = queue.pop_front() {
            for &(c, lp, lc) in &adj[p] {
                if !seen[c] {
                    seen[c] = true;
                    chain.push((p, c, lp, lc));
                    queue.push_back(c);
                }
            }
        }
        let mut params = IntegrationParameters::default();
        params.dt = 1.0 / 60.0;
        Some(Self {
            pipeline: PhysicsPipeline::new(),
            params,
            islands: IslandManager::new(),
            broad: BroadPhaseBvh::new(),
            narrow: NarrowPhase::new(),
            bodies,
            colliders,
            joints,
            multibody: MultibodyJointSet::new(),
            ccd: CCDSolver::new(),
            handles,
            offsets,
            inv_offsets,
            modes,
            bones: bone_ids.clone(),
            base_damping,
            gravity: Vec3::new(0.0, -98.0, 0.0),
            substeps: 2,
            snaps: 0,
            failed: false,
            chain,
            order: {
                let depth = |mut b: i32| {
                    let mut d = 0;
                    while b >= 0 && d < 512 {
                        b = m.bones[b as usize].parent;
                        d += 1;
                    }
                    d
                };
                let mut o: Vec<usize> = (0..bone_ids.len()).collect();
                o.sort_by_key(|&i| depth(bone_ids[i]));
                o
            },
            bone_parent: bone_ids.iter().map(|&b| if b >= 0 { m.bones[b as usize].parent } else { -1 }).collect(),
        })
    }

    pub fn body_count(&self) -> usize {
        self.handles.len()
    }

    /// Snap every body to its bone (after a jump in time).
    pub fn reset(&mut self, skel: &Skeleton) {
        for i in 0..self.handles.len() {
            let Some(h) = self.handles[i] else { continue };
            let bg = if self.bones[i] >= 0 { skel.states[self.bones[i] as usize].global } else { Mat4::IDENTITY };
            let p = to_pose(&(bg * self.offsets[i]));
            let b = &mut self.bodies[h];
            b.set_position(p, true);
            b.set_linvel(Vec3::ZERO, true);
            b.set_angvel(Vec3::ZERO, true);
        }
    }

    /// Bones moved by the simulation (not just following their bone).
    pub fn simulated_bones(&self) -> Vec<usize> {
        self.bones.iter().zip(self.modes.iter()).filter(|(b, m)| **b >= 0 && **m != RigidMode::FollowBone).map(|(b, _)| *b as usize).collect()
    }

    pub fn step(&mut self, skel: &mut Skeleton, dt: f32, cfg: &PhysicsCfg, time: f32) {
        if self.failed {
            return;
        }
        self.gravity = cfg.gravity_vec();
        // Drive bone-following bodies, and notice when the character jumped far this frame.
        let mut max_jump = 0.0f32;
        let mut sum = Vec3::ZERO;
        let mut count = 0.0;
        let mut targets = Vec::new();
        for i in 0..self.handles.len() {
            let Some(h) = self.handles[i] else { continue };
            if self.modes[i] == RigidMode::FollowBone && self.bones[i] >= 0 {
                let bg = skel.states[self.bones[i] as usize].global;
                let target = to_pose(&(bg * self.offsets[i]));
                let d = target.translation - self.bodies[h].translation();
                max_jump = max_jump.max(d.length());
                sum += d;
                count += 1.0;
                targets.push((h, target));
            }
        }
        let frame_scale = (dt * 30.0).max(0.25);
        if max_jump > cfg.big_move_dist * frame_scale && count > 0.0 {
            match cfg.big_move {
                BigMove::Snap => {
                    self.reset(skel);
                    self.snaps += 1;
                }
                BigMove::Soften => {
                    // carry simulated parts along with the average jump and calm them down
                    let avg = sum / count;
                    for i in 0..self.handles.len() {
                        let Some(h) = self.handles[i] else { continue };
                        if self.modes[i] != RigidMode::FollowBone {
                            let b = &mut self.bodies[h];
                            let mut pose = *b.position();
                            pose.translation += avg;
                            b.set_position(pose, true);
                            let v = b.linvel();
                            b.set_linvel(v * 0.3, true);
                        }
                    }
                    self.snaps += 1;
                }
                BigMove::Off => {}
            }
        }
        for (h, target) in targets {
            self.bodies[h].set_next_kinematic_position(target);
        }
        // Wind (with gusts) and air drag on simulated parts.
        let wind = cfg.wind_vec();
        for i in 0..self.handles.len() {
            let Some(h) = self.handles[i] else { continue };
            if self.modes[i] == RigidMode::FollowBone {
                continue;
            }
            let b = &mut self.bodies[h];
            b.reset_forces(false);
            if wind.length_squared() > 0.0 {
                let fi = i as f32;
                let gust = 1.0 + cfg.wind_gusts * (0.6 * (time * 1.7 + fi * 0.37).sin() + 0.4 * (time * 0.53 + fi * 1.3).sin());
                let m = b.mass();
                b.add_force(wind * gust.max(0.0) * m, true);
            }
            b.set_linear_damping(self.base_damping[i] + cfg.air_drag * 5.0);
        }
        let n = self.substeps.max(1);
        self.params.dt = (dt / n as f32).clamp(1.0 / 960.0, 1.0 / 30.0);
        let ok = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            for _ in 0..n {
                self.pipeline.step(
                    self.gravity,
                    &self.params,
                    &mut self.islands,
                    &mut self.broad,
                    &mut self.narrow,
                    &mut self.bodies,
                    &mut self.colliders,
                    &mut self.joints,
                    &mut self.multibody,
                    &mut self.ccd,
                    &(),
                    &(),
                );
            }
        }));
        if ok.is_err() {
            self.failed = true;
            return;
        }
        self.hold_joints();
        // Keep things from flying off: cap how fast simulated parts may move.
        for i in 0..self.handles.len() {
            let Some(h) = self.handles[i] else { continue };
            if self.modes[i] == RigidMode::FollowBone {
                continue;
            }
            let b = &mut self.bodies[h];
            let v = b.linvel();
            if v.length() > cfg.max_speed {
                b.set_linvel(v.normalize() * cfg.max_speed, false);
            }
            let w = b.angvel();
            if w.length() > 60.0 {
                b.set_angvel(w.normalize() * 60.0, false);
            }
            if !b.translation().is_finite() {
                b.set_linvel(Vec3::ZERO, false);
            }
        }
        // Write simulated transforms back to bones, parents first. "Physics + bone position"
        // parts take their rotation from the simulation but keep their place relative to their
        // parent bone (which may itself have just been moved by physics).
        let before: Vec<Mat4> = skel.states.iter().map(|s| s.global).collect();
        for k in 0..self.order.len() {
            let i = self.order[k];
            let Some(h) = self.handles[i] else { continue };
            if self.modes[i] == RigidMode::FollowBone || self.bones[i] < 0 {
                continue;
            }
            let bi = self.bones[i] as usize;
            let body = from_pose(self.bodies[h].position());
            let mut g = body * self.inv_offsets[i];
            if self.modes[i] == RigidMode::PhysicsWithBonePosition {
                let par = self.bone_parent[i];
                g.w_axis = if par >= 0 && (par as usize) < before.len() {
                    let par = par as usize;
                    let local = before[par].inverse().transform_point3(before[bi].w_axis.truncate());
                    skel.states[par].global.transform_point3(local).extend(1.0)
                } else {
                    before[bi].w_axis
                };
            }
            if !g.is_finite() {
                continue;
            }
            skel.states[bi].global = g;
            skel.states[bi].physics_driven = true;
        }
    }

    /// The engine solves joints approximately, so in long chains (twin tails, ribbons) small
    /// errors add up and the tip drifts away from its root during fast moves. Pull every
    /// linked piece back so its joint point meets its parent's again, root first.
    fn hold_joints(&mut self) {
        for k in 0..self.chain.len() {
            let (p, c, lp, lc) = self.chain[k];
            let (Some(hp), Some(hc)) = (self.handles[p], self.handles[c]) else { continue };
            let wp = from_pose(self.bodies[hp].position());
            let target = wp.transform_point3(lp);
            let body = &mut self.bodies[hc];
            let mut pose = *body.position();
            let cur = from_pose(&pose).transform_point3(lc);
            let d = target - cur;
            // Small errors are the engine's normal give; only step in when a piece has really
            // drifted (tested on 20+ community models: 0.3 units keeps every chain together
            // without fighting skirts and helper-link rigs).
            if !d.is_finite() || d.length() < 0.3 {
                continue;
            }
            pose.translation += d;
            body.set_position(pose, false);
        }
    }

    /// Debug shapes for the viewport: (world transform, shape, size, dynamic).
    pub fn debug_shapes(&self, m: &ModelData) -> Vec<(Mat4, Shape, Vec3, bool)> {
        let mut v = Vec::new();
        for (i, rb) in m.rigid_bodies.iter().enumerate() {
            if let Some(Some(h)) = self.handles.get(i) {
                v.push((from_pose(self.bodies[*h].position()), rb.shape, rb.size, rb.mode != RigidMode::FollowBone));
            }
        }
        v
    }
}
