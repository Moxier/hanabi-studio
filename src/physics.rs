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
    /// Multiplies every part's damping (live).
    pub damping_mul: f32,
    /// Multiplies every joint spring (live).
    pub spring_mul: f32,
    /// Fixed physics step in seconds; 0 = follow the frame rate (split into sub-steps).
    pub fixed_step: f32,
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
            damping_mul: 1.0,
            spring_mul: 1.0,
            // Fixed 1/60 s steps: steadier and less likely to break than following the frame rate.
            fixed_step: 1.0 / 60.0,
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
    base_ang_damping: Vec<f32>,
    /// Each simulated joint: engine handle, model joint index, the two bodies, the joint point
    /// in body A's space, and its springs (lin xyz, ang xyz) as read from the model.
    joint_info: Vec<(ImpulseJointHandle, usize, usize, usize, Vec3, [f32; 6])>,
    applied_spring_mul: f32,
    accum: f32,
    pub gravity: Vec3,
    pub substeps: u32,
    /// Times the simulation was snapped back after a big move (shown in the Physics window).
    pub snaps: u32,
    /// How far (in model units) a simulated part may drift from where its bone would hold it
    /// before it counts as flown off; about 1.5x the size of the model's physics.
    guard_dist: f32,
    /// How far parts drift compared with the blow-up limit (95th percentile; 1.0 = flown off).
    pub last_score: f32,
    /// Parts held by at least one joint (loose parts don't count towards the score).
    jointed: Vec<bool>,
    /// Times parts flew off and were put back on their bones.
    pub blowups: u32,
    /// The part that drifted furthest in the last step.
    pub worst_body: usize,
    /// For each body: its bone's rest offset from the parent bone, when both bone and parent
    /// are moved by jointed bodies (a link inside a hair / skirt chain); used to spot torn or
    /// flipped chains.
    chain_rest: Vec<Option<(usize, usize, Vec3)>>,
    /// Steps in a row with torn / flipped chain links (so brief swings don't count).
    /// For each simulated part: (its bone, the nearest bone above it that physics doesn't move
    /// — the hip for a skirt, the head for hair — and the rest offset between them). A whole
    /// skirt swinging up past the hip keeps every link straight, so flips are judged against
    /// this anchor.
    chain_anchor: Vec<(usize, usize, Vec3, bool)>,
    tear_run: u32,
    flip_run: u32,
    /// Largest turn (degrees) of a simulated part away from its rest direction, seen from its
    /// anchor, in the last step; and the parts turned past the flip limit.
    pub max_flip_deg: f32,
    pub flipped_bones: Vec<usize>,
    /// Times a skirt / hair chain stayed turned over for half a second (reported, not reset:
    /// a crouch or a raised arm can do this on purpose).
    pub flip_events: u32,
    /// How far the most-lifted skirt hem has risen: 0 = hanging as at rest, 1 = up at the hip
    /// bone (0.6 and above counts as flipped).
    pub hem_height: f32,
    /// The current flip has been reported (cleared when it has settled again).
    flip_reported: bool,
    /// Parts in the latest such flip.
    pub last_flip: Vec<usize>,
    /// Chain links torn apart / turned inside out in the last step.
    pub torn: usize,
    pub flipped: usize,
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

/// A simulated part counts as flipped when it has turned more than this far from its rest
/// direction, seen from the body part it hangs from.
const FLIP_DEG: f32 = 150.0;
/// A skirt counts as flipped up when its hem has lifted this share of the way to the hip bone.
const HEM_LIFT: f32 = 0.6;

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
        let mut base_ang_damping = Vec::new();
        // Each body's rest-pose transform (from the model file). Joint anchors must be worked out
        // in the rest pose: the bodies themselves start at the current (possibly mid-dance) pose,
        // and mixing the two put every joint in the wrong place when physics was rebuilt at a
        // frame other than the rest pose.
        let mut rest_worlds: Vec<Mat4> = Vec::with_capacity(m.rigid_bodies.len());
        for rb in &m.rigid_bodies {
            let rot = Quat::from_euler(EulerRot::YXZ, rb.rotation.y, rb.rotation.x, rb.rotation.z);
            let world = Mat4::from_rotation_translation(rot, rb.position);
            // Same rounding as the engine's own pose, so building at the rest pose gives exactly the
            // joint frames it always did.
            rest_worlds.push(from_pose(&to_pose(&world)));
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
            base_ang_damping.push(damping(rb.angular_damping));
        }
        let mut joint_info = Vec::new();
        let mut joints = ImpulseJointSet::new();
        let mut links: Vec<(usize, usize, Vec3, Vec3)> = Vec::new();

        for (ji, j) in m.joints.iter().enumerate() {
            let (a, b) = (j.a, j.b);
            if a < 0 || b < 0 || a as usize >= handles.len() || b as usize >= handles.len() || a == b {
                continue;
            }
            let (ha, hb) = (handles[a as usize].unwrap(), handles[b as usize].unwrap());
            let rot = Quat::from_euler(EulerRot::YXZ, j.rotation.y, j.rotation.x, j.rotation.z);
            let jw = Mat4::from_rotation_translation(rot, j.position);
            let pa = rest_worlds[a as usize];
            let pb = rest_worlds[b as usize];
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
            let jh = joints.insert(ha, hb, gj, true);
            let springs = [j.spring_lin.x, j.spring_lin.y, j.spring_lin.z, j.spring_ang.x, j.spring_ang.y, j.spring_ang.z];
            let free: Vec<bool> = (0..6).map(|k| !locked.contains(if k < 3 { lin_bits[k] } else { ang_bits[k - 3] })).collect();
            let springs: [f32; 6] = std::array::from_fn(|k| if free[k] { springs[k].max(0.0) } else { 0.0 });
            joint_info.push((jh, ji, a as usize, b as usize, (pa.inverse() * jw).w_axis.truncate(), springs));
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
        let mut jointed = vec![false; bone_ids.len()];
        for (_, _, a, b, _, _) in &joint_info {
            jointed[*a] = true;
            jointed[*b] = true;
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
            base_ang_damping,
            joint_info,
            applied_spring_mul: 1.0,
            accum: 0.0,
            gravity: Vec3::new(0.0, -98.0, 0.0),
            substeps: 2,
            snaps: 0,
            guard_dist: {
                let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
                for rb in &m.rigid_bodies {
                    lo = lo.min(rb.position);
                    hi = hi.max(rb.position);
                }
                ((hi - lo).max_element() * 1.5).max(10.0)
            },
            last_score: 0.0,
            jointed: jointed.clone(),
            blowups: 0,
            worst_body: 0,
            chain_rest: {
                // bone -> is it moved by a jointed, simulated body?
                let mut sim_bone = vec![false; m.bones.len()];
                for (i, rb) in m.rigid_bodies.iter().enumerate() {
                    if rb.mode != RigidMode::FollowBone && rb.bone >= 0 && (rb.bone as usize) < m.bones.len() && jointed[i] {
                        sim_bone[rb.bone as usize] = true;
                    }
                }
                m.rigid_bodies
                    .iter()
                    .enumerate()
                    .map(|(i, rb)| {
                        if rb.mode == RigidMode::FollowBone || !jointed[i] || rb.bone < 0 || rb.bone as usize >= m.bones.len() {
                            return None;
                        }
                        let b = rb.bone as usize;
                        let par = m.bones[b].parent;
                        if par < 0 || !sim_bone[par as usize] {
                            return None;
                        }
                        let off = m.bones[b].position - m.bones[par as usize].position;
                        (off.length() > 0.05).then_some((b, par as usize, off))
                    })
                    .collect()
            },
            chain_anchor: {
                let mut sim_bone = vec![false; m.bones.len()];
                for (i, rb) in m.rigid_bodies.iter().enumerate() {
                    if rb.mode != RigidMode::FollowBone && rb.bone >= 0 && (rb.bone as usize) < m.bones.len() && jointed[i] {
                        sim_bone[rb.bone as usize] = true;
                    }
                }
                // Hip anchors: the bones the feet hang from (found as the ancestors of the lowest
                // bones the physics doesn't move). Arms that hang down in a model's rest pose are
                // not hips. A skirt there must never end up above its anchor.
                let mut hip = vec![false; m.bones.len()];
                let min_y = m.bones.iter().enumerate().filter(|(i, _)| !sim_bone[*i]).map(|(_, b)| b.position.y).fold(f32::MAX, f32::min);
                for (d, bd) in m.bones.iter().enumerate() {
                    if sim_bone[d] || bd.position.y > min_y + 2.0 {
                        continue;
                    }
                    let mut a = bd.parent;
                    let mut guard = 0;
                    while a >= 0 && guard < 256 {
                        hip[a as usize] = true;
                        a = m.bones[a as usize].parent;
                        guard += 1;
                    }
                }
                let mut v = Vec::new();
                for (b, is_sim) in sim_bone.iter().enumerate() {
                    if !is_sim {
                        continue;
                    }
                    let mut a = m.bones[b].parent;
                    let mut guard = 0;
                    while a >= 0 && sim_bone[a as usize] && guard < 256 {
                        a = m.bones[a as usize].parent;
                        guard += 1;
                    }
                    if a < 0 {
                        continue;
                    }
                    let off = m.bones[b].position - m.bones[a as usize].position;
                    // Parts right next to their anchor turn a lot without meaning anything.
                    if off.length() > 0.5 {
                        v.push((b, a as usize, off, hip[a as usize]));
                    }
                }
                v
            },
            max_flip_deg: 0.0,
            flipped_bones: Vec::new(),
            flip_events: 0,
            hem_height: 0.0,
            flip_reported: false,
            last_flip: Vec::new(),
            tear_run: 0,
            flip_run: 0,
            torn: 0,
            flipped: 0,
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
        self.accum = 0.0;
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
        // Joint springs follow the stiffness multiplier live.
        if (cfg.spring_mul - self.applied_spring_mul).abs() > 1e-5 {
            let axes = [JointAxis::LinX, JointAxis::LinY, JointAxis::LinZ, JointAxis::AngX, JointAxis::AngY, JointAxis::AngZ];
            for (jh, _, _, _, _, springs) in &self.joint_info {
                if let Some(j) = self.joints.get_mut(*jh, true) {
                    for k in 0..6 {
                        if springs[k] > 0.0 {
                            j.data.set_motor(axes[k], 0.0, 0.0, springs[k] * cfg.spring_mul.max(0.0), 0.1);
                        }
                    }
                }
            }
            self.applied_spring_mul = cfg.spring_mul;
        }
        let fixed = cfg.fixed_step > 0.0;
        let starts: Vec<Pose> = if fixed { targets.iter().map(|(h, _)| *self.bodies[*h].position()).collect() } else { Vec::new() };
        if !fixed {
            for (h, target) in &targets {
                self.bodies[*h].set_next_kinematic_position(*target);
            }
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
            b.set_linear_damping(self.base_damping[i] * cfg.damping_mul + cfg.air_drag * 5.0);
            b.set_angular_damping(self.base_ang_damping[i] * cfg.damping_mul);
        }
        // Either split this frame's time into sub-steps, or advance in equal fixed steps
        // (carrying left-over time to the next frame) and move bone-following parts evenly.
        let n = if fixed {
            let h = cfg.fixed_step.clamp(1.0 / 960.0, 1.0 / 20.0);
            self.params.dt = h;
            self.accum += dt.clamp(0.0, 0.25);
            let mut n = (self.accum / h + 1e-4).floor() as u32;
            if n > 32 {
                n = 32;
                self.accum = n as f32 * h;
            }
            self.accum = (self.accum - n as f32 * h).max(0.0);
            n
        } else {
            let n = self.substeps.max(1);
            self.params.dt = (dt / n as f32).clamp(1.0 / 960.0, 1.0 / 30.0);
            n
        };
        let ok = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            for k in 0..n {
                if fixed {
                    let t = (k + 1) as f32 / n as f32;
                    for ((hd, target), start) in targets.iter().zip(&starts) {
                        let pose = Pose::from_parts(start.translation.lerp(target.translation, t), start.rotation.slerp(target.rotation, t));
                        self.bodies[*hd].set_next_kinematic_position(pose);
                    }
                }
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
        // Blow-up guard. Each simulated part is compared with where its bone would hold it.
        // A part that wandered past the limit on its own (a loose earring, an odd ribbon) is
        // quietly put back; when many parts fly off at once, or numbers break, the whole model
        // is put back on its bones and the blow-up is counted (the Physics window offers Undo).
        let mut ratios: Vec<f32> = Vec::new();
        let mut far = 0usize;
        let mut broken = false;
        let mut worst_i = 0;
        let mut worst = 0.0f32;
        for i in 0..self.handles.len() {
            let Some(h) = self.handles[i] else { continue };
            if self.modes[i] == RigidMode::FollowBone || self.bones[i] < 0 {
                continue;
            }
            let target = (skel.states[self.bones[i] as usize].global * self.offsets[i]).w_axis.truncate();
            let d = (self.bodies[h].translation() - target).length();
            if !d.is_finite() {
                broken = true;
                continue;
            }
            if d > worst {
                worst = d;
                worst_i = i;
            }
            if d > self.guard_dist {
                far += 1;
                // Put this one part back.
                let bg = skel.states[self.bones[i] as usize].global;
                let body = &mut self.bodies[h];
                body.set_position(to_pose(&(bg * self.offsets[i])), true);
                body.set_linvel(Vec3::ZERO, true);
                body.set_angvel(Vec3::ZERO, true);
            }
            if self.jointed[i] {
                ratios.push(d / self.guard_dist);
            }
        }
        self.worst_body = worst_i;
        // Score: the 95th percentile, so one or two odd parts don't decide it.
        ratios.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        self.last_score = ratios.get(((ratios.len() as f32 * 0.95) as usize).min(ratios.len().saturating_sub(1))).copied().unwrap_or(0.0);
        let many = (ratios.len() / 10).max(3);
        if broken || far >= many {
            self.reset(skel);
            self.blowups += 1;
            return;
        }
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
        self.check_chains(skel);
    }

    /// Second blow-up check, on the chains themselves: links pulled far from their parent
    /// (torn) or turned more than 150° from their rest direction (a skirt or hair chain flipped
    /// inside out and stuck). If it lasts (10 steps torn / 30 steps flipped), everything is put
    /// back on its bones and counted as a blow-up. Parts the physics drift check misses, such
    /// as a whole skirt flipping, are caught here.
    /// Parts hanging from the hips (skirts, coat tails).
    pub fn hip_parts(&self) -> usize {
        self.chain_anchor.iter().filter(|c| c.3).count()
    }

    fn check_chains(&mut self, skel: &mut Skeleton) {
        let (mut torn, mut flipped, mut links) = (0usize, 0usize, 0usize);
        for c in self.chain_rest.iter().flatten() {
            let (b, par, rest) = *c;
            let local = skel.states[par].global.inverse().transform_point3(skel.states[b].global.w_axis.truncate());
            if !local.is_finite() {
                torn += 1;
                continue;
            }
            links += 1;
            let rl = rest.length();
            if local.length() - rl > (rl * 2.0).max(2.0) {
                torn += 1;
            } else if local.normalize_or_zero().dot(rest / rl) < -0.866 {
                flipped += 1;
            }
        }
        // Flips. (1) A skirt over the hip: for each group of parts hanging from the hips, the
        // lower half (the hem) is averaged; a hem higher than the hip bone means the skirt has
        // flipped up. Averaging the hem means one loose piece can't trigger it. (2) Parts on
        // any anchor turned right round (more than 150 degrees).
        let mut max_deg = 0.0f32;
        let mut turned = Vec::new();
        let mut groups: std::collections::BTreeMap<usize, Vec<(usize, f32, f32)>> = Default::default();
        for &(b, anc, rest, hip) in &self.chain_anchor {
            let local = skel.states[anc].global.inverse().transform_point3(skel.states[b].global.w_axis.truncate());
            if !local.is_finite() || local.length_squared() < 1e-8 {
                continue;
            }
            let deg = local.normalize().dot(rest.normalize()).clamp(-1.0, 1.0).acos().to_degrees();
            max_deg = max_deg.max(deg);
            if deg > FLIP_DEG {
                turned.push(b);
            }
            if hip && rest.y < -0.5 {
                let dy = skel.states[b].global.w_axis.y - skel.states[anc].global.w_axis.y;
                groups.entry(anc).or_default().push((b, rest.y, dy));
            }
        }
        // How far each hem has lifted: 0 = hanging as at rest, 1 = up at the hip bone.
        let mut hem_flipped: Vec<usize> = Vec::new();
        let mut lift_max = 0.0f32;
        for g in groups.values_mut() {
            if g.len() < 6 {
                continue;
            }
            g.sort_by(|x, y| x.1.partial_cmp(&y.1).unwrap_or(std::cmp::Ordering::Equal));
            let hem = &g[..(g.len() / 2).max(3)];
            let rest_avg = hem.iter().map(|h| h.1).sum::<f32>() / hem.len() as f32;
            let now_avg = hem.iter().map(|h| h.2).sum::<f32>() / hem.len() as f32;
            if rest_avg > -0.5 {
                continue;
            }
            let lift = 1.0 - now_avg / rest_avg;
            lift_max = lift_max.max(lift);
            if lift > HEM_LIFT {
                hem_flipped.extend(hem.iter().filter(|h| h.2 > h.1 * (1.0 - HEM_LIFT)).map(|h| h.0));
            }
        }
        self.hem_height = lift_max;
        self.max_flip_deg = max_deg;
        let need_turned = (self.chain_anchor.len() / 10).max(3);
        let flipping = !hem_flipped.is_empty() || turned.len() >= need_turned;
        self.flipped_bones = if !hem_flipped.is_empty() { hem_flipped } else if flipping { turned } else { Vec::new() };
        self.torn = torn;
        // Links flipped against their neighbour count too when several are (a chain folded).
        self.flipped = self.flipped_bones.len().max(if flipped >= (links / 20).max(2) { flipped } else { 0 });
        let need = (links / 20).max(2);
        self.tear_run = if torn >= need { self.tear_run + 1 } else { 0 };
        // The timer fades rather than restarting, so a skirt that stays up but wobbles adds up.
        self.flip_run = if flipping { self.flip_run + 1 } else { self.flip_run.saturating_sub(2) };
        // Torn chains are a blow-up: put everything back on its bones.
        if self.tear_run >= 10 {
            self.tear_run = 0;
            self.flip_run = 0;
            self.reset(skel);
            self.blowups += 1;
        }
        // A flip held for half a second is reported once (the Physics window shows it).
        if self.flip_run >= 30 && !self.flip_reported {
            self.flip_events += 1;
            self.last_flip = self.flipped_bones.clone();
            self.flip_reported = true;
        }
        if self.flip_run == 0 {
            self.flip_reported = false;
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
    pub fn debug_shapes(&self, m: &ModelData) -> Vec<(usize, Mat4, Shape, Vec3, RigidMode)> {
        let mut v = Vec::new();
        for (i, rb) in m.rigid_bodies.iter().enumerate() {
            if let Some(Some(h)) = self.handles.get(i) {
                v.push((i, from_pose(self.bodies[*h].position()), rb.shape, rb.size, rb.mode));
            }
        }
        v
    }

    /// Joints for the view: (model joint index, joint point, centre of body A, centre of body B).
    pub fn debug_joints(&self) -> Vec<(usize, Vec3, Vec3, Vec3)> {
        self.joint_info
            .iter()
            .filter_map(|(_, ji, a, b, la, _)| {
                let pa = from_pose(self.bodies[self.handles[*a]?].position());
                let pb = from_pose(self.bodies[self.handles[*b]?].position());
                Some((*ji, pa.transform_point3(*la), pa.w_axis.truncate(), pb.w_axis.truncate()))
            })
            .collect()
    }
}

/// Quick stability test of a model's physics: hold the current pose, simulate two seconds,
/// and report the worst drift as a share of the blow-up limit (1.0 = it would fly off), plus
/// whether it blew up or failed.
pub fn stability_test(m: &ModelData, skel: &Skeleton, cfg: &PhysicsCfg, substeps: u32) -> (f32, bool) {
    let Some(mut ph) = Physics::new(m, skel) else { return (0.0, false) };
    ph.substeps = substeps;
    let mut sk = skel.clone();
    ph.reset(&sk);
    let mut worst = 0.0f32;
    for k in 0..120 {
        let base = skel.clone();
        sk.states.clone_from(&base.states);
        ph.step(&mut sk, 1.0 / 60.0, cfg, k as f32 / 60.0);
        if ph.failed {
            return (f32::INFINITY, true);
        }
        worst = worst.max(ph.last_score);
        if std::env::var_os("HST_STAB").is_some() && (k % 30 == 0 || ph.blowups > 0) {
            eprintln!("step {k}: score {:.2}, furthest part #{}", ph.last_score, ph.worst_body);
        }
        if ph.blowups > 0 {
            return (worst.max(1.0), true);
        }
    }
    (worst, false)
}
