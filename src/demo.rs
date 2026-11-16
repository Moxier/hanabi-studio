//! A procedurally built demo character and dance so the studio shows something with no files.
//! Everything here is generated in code (no external assets).

use crate::model::*;
use crate::motion::*;
use glam::{Quat, Vec2, Vec3, Vec4};

struct Builder {
    m: ModelData,
}

impl Builder {
    fn bone(&mut self, name: &str, pos: Vec3, parent: i32) -> i32 {
        let mut b = Bone::new(name, pos, parent);
        b.name_en = name.to_string();
        self.m.bones.push(b);
        (self.m.bones.len() - 1) as i32
    }

    /// Add a tapered capsule-like tube from a to b, skinned between two bones.
    #[allow(clippy::too_many_arguments)]
    fn tube(&mut self, a: Vec3, b: Vec3, ra: f32, rb: f32, bone_a: i32, bone_b: i32, mat: usize, segs: usize, rings: usize) {
        let base = self.m.vertices.len() as u32;
        let axis = (b - a).normalize_or_zero();
        let up = if axis.y.abs() > 0.9 { Vec3::X } else { Vec3::Y };
        let u = axis.cross(up).normalize();
        let v = axis.cross(u).normalize();
        // rings along the axis plus hemispherical caps
        let cap = 3;
        let total_rings = rings + 2 * cap;
        for ri in 0..=total_rings {
            let (center, radius, t) = if ri < cap {
                let ang = (1.0 - ri as f32 / cap as f32) * std::f32::consts::FRAC_PI_2;
                (a - axis * ra * ang.sin(), ra * ang.cos(), 0.0)
            } else if ri > rings + cap {
                let k = (ri - rings - cap) as f32 / cap as f32;
                let ang = k * std::f32::consts::FRAC_PI_2;
                (b + axis * rb * ang.sin(), rb * ang.cos(), 1.0)
            } else {
                let t = (ri - cap) as f32 / rings as f32;
                (a.lerp(b, t), ra + (rb - ra) * t, t)
            };
            for si in 0..=segs {
                let ang = si as f32 / segs as f32 * std::f32::consts::TAU;
                let dir = u * ang.cos() + v * ang.sin();
                let pos = center + dir * radius.max(0.0005);
                let mut n = dir;
                if ri < cap {
                    n = (pos - a).normalize_or(dir);
                } else if ri > rings + cap {
                    n = (pos - b).normalize_or(dir);
                }
                let w = smooth(t);
                self.m.vertices.push(Vertex {
                    pos,
                    normal: n,
                    uv: Vec2::new(si as f32 / segs as f32, ri as f32 / total_rings as f32),
                    bones: [bone_a, bone_b, 0, 0],
                    weights: [1.0 - w, w, 0.0, 0.0],
                    edge_scale: 1.0,
                    sdef: None,
                });
            }
        }
        let stride = segs as u32 + 1;
        let mut idx = Vec::new();
        for ri in 0..total_rings as u32 {
            for si in 0..segs as u32 {
                let i0 = base + ri * stride + si;
                let i1 = i0 + 1;
                let i2 = i0 + stride;
                let i3 = i2 + 1;
                idx.extend_from_slice(&[i0, i1, i2, i1, i3, i2]);
            }
        }
        self.push_indices(mat, idx);
    }

    fn sphere(&mut self, c: Vec3, r: Vec3, bone: i32, mat: usize, segs: usize, rings: usize, morph_scale: Option<&mut Vec<(u32, Vec3)>>) {
        let base = self.m.vertices.len() as u32;
        let mut morph = morph_scale;
        for ri in 0..=rings {
            let phi = ri as f32 / rings as f32 * std::f32::consts::PI;
            for si in 0..=segs {
                let th = si as f32 / segs as f32 * std::f32::consts::TAU;
                let n = Vec3::new(phi.sin() * th.cos(), phi.cos(), phi.sin() * th.sin());
                let pos = c + n * r;
                let vi = self.m.vertices.len() as u32;
                if let Some(mv) = morph.as_deref_mut() {
                    // "smile": lift the lower front of the face a bit and widen.
                    if n.z < -0.3 && n.y < 0.1 && n.y > -0.7 {
                        let k = (-n.z - 0.3) * (0.1 - n.y);
                        mv.push((vi, Vec3::new(n.x * 0.15, 0.12, 0.0) * k));
                    }
                }
                self.m.vertices.push(Vertex {
                    pos,
                    normal: (n / r).normalize(),
                    uv: Vec2::new(si as f32 / segs as f32, ri as f32 / rings as f32),
                    bones: [bone, 0, 0, 0],
                    weights: [1.0, 0.0, 0.0, 0.0],
                    edge_scale: 1.0,
                    sdef: None,
                });
            }
        }
        let stride = segs as u32 + 1;
        let mut idx = Vec::new();
        for ri in 0..rings as u32 {
            for si in 0..segs as u32 {
                let i0 = base + ri * stride + si;
                let i1 = i0 + 1;
                let i2 = i0 + stride;
                let i3 = i2 + 1;
                idx.extend_from_slice(&[i0, i1, i2, i1, i3, i2]);
            }
        }
        self.push_indices(mat, idx);
    }

    fn push_indices(&mut self, mat: usize, idx: Vec<u32>) {
        // Materials are laid out contiguously: insert into the right slot.
        let mut start = 0u32;
        for m in &self.m.materials[..mat] {
            start += m.index_count;
        }
        let end = start + self.m.materials[mat].index_count;
        let n = idx.len() as u32;
        let tail: Vec<u32> = self.m.indices.split_off(end as usize);
        self.m.indices.extend(idx);
        self.m.indices.extend(tail);
        self.m.materials[mat].index_count += n;
    }
}

fn smooth(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn mat(name: &str, color: Vec3, toon: u8) -> Material {
    Material {
        name: name.into(),
        name_en: name.into(),
        diffuse: color.extend(1.0),
        ambient: color * 0.38,
        specular: Vec3::splat(0.25),
        specular_power: 20.0,
        edge_size: 0.8,
        edge_color: Vec4::new(0.12, 0.08, 0.14, 1.0),
        toon: Toon::Shared(toon),
        ..Default::default()
    }
}

pub fn mannequin() -> ModelData {
    let mut b = Builder { m: ModelData { name: "Demo Mannequin".into(), name_en: "Demo Mannequin".into(), ..Default::default() } };
    b.m.comment = "Procedurally generated demo character. Free to use.".into();
    b.m.materials.push(mat("skin", Vec3::new(0.98, 0.82, 0.74), 1));
    b.m.materials.push(mat("suit", Vec3::new(0.24, 0.42, 0.78), 2));
    b.m.materials.push(mat("accent", Vec3::new(0.9, 0.38, 0.55), 3));
    b.m.materials.push(mat("hair", Vec3::new(0.28, 0.2, 0.42), 4));
    b.m.materials.push(mat("eyes", Vec3::new(0.08, 0.08, 0.12), 0));
    b.m.materials[4].edge = false;

    // Skeleton (MMD-like structure, English names)
    let root = b.bone("root", Vec3::ZERO, -1);
    b.m.bones[root as usize].movable = true;
    let center = b.bone("center", Vec3::new(0.0, 8.0, 0.0), root);
    b.m.bones[center as usize].movable = true;
    let groove = b.bone("groove", Vec3::new(0.0, 8.2, 0.0), center);
    b.m.bones[groove as usize].movable = true;
    let waist = b.bone("waist", Vec3::new(0.0, 10.0, 0.0), groove);
    let upper = b.bone("upper body", Vec3::new(0.0, 10.6, 0.0), waist);
    let upper2 = b.bone("upper body 2", Vec3::new(0.0, 12.2, 0.0), upper);
    let neck = b.bone("neck", Vec3::new(0.0, 14.6, 0.0), upper2);
    let head = b.bone("head", Vec3::new(0.0, 15.4, 0.0), neck);
    let lower = b.bone("lower body", Vec3::new(0.0, 10.6, 0.0), waist);

    let mut arms = Vec::new();
    for (side, s) in [("_L", 1.0f32), ("_R", -1.0f32)] {
        let sh = b.bone(&format!("shoulder{side}"), Vec3::new(0.6 * s, 14.0, 0.0), upper2);
        let arm = b.bone(&format!("arm{side}"), Vec3::new(1.7 * s, 13.8, 0.0), sh);
        let elbow = b.bone(&format!("elbow{side}"), Vec3::new(4.1 * s, 12.0, 0.0), arm);
        let wrist = b.bone(&format!("wrist{side}"), Vec3::new(6.1 * s, 10.4, 0.0), elbow);
        let tip = b.bone(&format!("hand tip{side}"), Vec3::new(6.9 * s, 9.8, 0.0), wrist);
        b.m.bones[tip as usize].visible = false;
        arms.push((sh, arm, elbow, wrist, tip, s));
    }
    let mut legs = Vec::new();
    for (side, s) in [("_L", 1.0f32), ("_R", -1.0f32)] {
        let leg = b.bone(&format!("leg{side}"), Vec3::new(1.0 * s, 9.6, 0.0), lower);
        let knee = b.bone(&format!("knee{side}"), Vec3::new(1.05 * s, 5.4, 0.0), leg);
        let ankle = b.bone(&format!("ankle{side}"), Vec3::new(1.1 * s, 1.2, 0.0), knee);
        let toe = b.bone(&format!("toe{side}"), Vec3::new(1.1 * s, 0.2, -1.6), ankle);
        b.m.bones[toe as usize].visible = false;
        legs.push((leg, knee, ankle, toe, s));
    }
    // Leg IK bones
    for (i, (side, s)) in [("_L", 1.0f32), ("_R", -1.0f32)].iter().enumerate() {
        let (leg, knee, ankle, _toe, _) = legs[i];
        let ik = b.bone(&format!("leg IK{side}"), Vec3::new(1.1 * s, 1.2, 0.0), root);
        let bm = &mut b.m.bones[ik as usize];
        bm.movable = true;
        bm.level = 0;
        bm.ik = Some(Ik {
            target: ankle,
            iterations: 40,
            limit_angle: 2.0,
            links: vec![
                IkLink { bone: knee, limit: Some((Vec3::new(-3.1, 0.0, 0.0), Vec3::new(-0.008, 0.0, 0.0))) },
                IkLink { bone: leg, limit: None },
            ],
        });
    }
    // Ponytail (physics chain)
    let mut hair = vec![head];
    for k in 0..4 {
        let h = b.bone(&format!("hair {k}"), Vec3::new(0.0, 17.2 - k as f32 * 1.5, 1.4 + k as f32 * 0.25), *hair.last().unwrap());
        hair.push(h);
    }

    // Bones the user may not see are fine; tails for nicer display
    for i in 0..b.m.bones.len() {
        b.m.bones[i].tail_bone = -1;
    }

    // Body geometry
    let skin = 0;
    let suit = 1;
    let accent = 2;
    let hair_m = 3;
    let eyes = 4;
    let mut smile = Vec::new();
    b.sphere(Vec3::new(0.0, 16.6, 0.0), Vec3::new(1.55, 1.75, 1.6), head, skin, 20, 14, Some(&mut smile));
    b.sphere(Vec3::new(0.0, 17.1, 0.25), Vec3::new(1.68, 1.6, 1.62), head, hair_m, 20, 12, None);
    b.sphere(Vec3::new(0.55, 16.75, -1.38), Vec3::new(0.22, 0.32, 0.12), head, eyes, 8, 6, None);
    b.sphere(Vec3::new(-0.55, 16.75, -1.38), Vec3::new(0.22, 0.32, 0.12), head, eyes, 8, 6, None);
    b.tube(Vec3::new(0.0, 14.4, 0.0), Vec3::new(0.0, 15.3, 0.0), 0.5, 0.45, neck, head, skin, 10, 2);
    b.tube(Vec3::new(0.0, 10.4, 0.0), Vec3::new(0.0, 13.6, 0.0), 1.25, 1.55, upper, upper2, suit, 16, 5);
    b.tube(Vec3::new(0.0, 8.9, 0.0), Vec3::new(0.0, 10.6, 0.0), 1.35, 1.2, lower, upper, accent, 16, 3);
    for &(_sh, arm, elbow, wrist, tip, s) in &arms {
        let pa = b.m.bones[arm as usize].position;
        let pe = b.m.bones[elbow as usize].position;
        let pw = b.m.bones[wrist as usize].position;
        let pt = b.m.bones[tip as usize].position;
        b.tube(pa, pe, 0.55, 0.45, arm, elbow, suit, 10, 4);
        b.tube(pe, pw, 0.45, 0.35, elbow, wrist, skin, 10, 4);
        b.tube(pw, pt, 0.38, 0.3, wrist, wrist, skin, 8, 1);
        let _ = s;
    }
    for &(leg, knee, ankle, toe, _s) in &legs {
        let pl = b.m.bones[leg as usize].position;
        let pk = b.m.bones[knee as usize].position;
        let pa = b.m.bones[ankle as usize].position;
        let pt = b.m.bones[toe as usize].position;
        b.tube(pl, pk, 0.8, 0.6, leg, knee, suit, 12, 5);
        b.tube(pk, pa, 0.58, 0.42, knee, ankle, suit, 12, 5);
        b.tube(pa, pt + Vec3::new(0.0, 0.2, 0.0), 0.5, 0.42, ankle, ankle, accent, 10, 2);
    }
    for k in 0..4 {
        let pa = b.m.bones[hair[k + 1] as usize].position;
        let pb = if k + 2 < hair.len() { b.m.bones[hair[k + 2] as usize].position } else { pa + Vec3::new(0.0, -1.5, 0.3) };
        b.tube(pa, pb, 0.5 - k as f32 * 0.08, 0.42 - k as f32 * 0.08, hair[k + 1], if k + 2 < hair.len() { hair[k + 2] } else { hair[k + 1] }, hair_m, 8, 2);
    }

    b.m.morphs.push(Morph { name: "smile".into(), name_en: "smile".into(), panel: 3, kind: MorphKind::Vertex(smile) });
    b.m.morphs.push(Morph {
        name: "blush".into(),
        name_en: "blush".into(),
        panel: 4,
        kind: MorphKind::Material(vec![MaterialMorphOffset {
            material: 0,
            additive: true,
            diffuse: Vec4::new(0.12, -0.05, -0.04, 0.0),
            specular: Vec3::ZERO,
            specular_power: 0.0,
            ambient: Vec3::new(0.1, -0.03, -0.03),
            edge_color: Vec4::ZERO,
            edge_size: 0.0,
            texture_tint: Vec4::ZERO,
            sphere_tint: Vec4::ZERO,
            toon_tint: Vec4::ZERO,
        }]),
    });

    // Physics: head follows bone, hair simulated
    let mut prev_rb = -1;
    for (k, &hb) in hair.iter().enumerate() {
        let p = b.m.bones[hb as usize].position;
        let is_head = k == 0;
        b.m.rigid_bodies.push(RigidBody {
            name: format!("rb {k}"),
            bone: hb,
            group: if is_head { 0 } else { 1 },
            mask: if is_head { 0xFFFD } else { 0xFFFC },
            shape: if is_head { Shape::Sphere } else { Shape::Capsule },
            size: if is_head { Vec3::new(1.6, 0.0, 0.0) } else { Vec3::new(0.4, 1.0, 0.0) },
            position: if is_head { Vec3::new(0.0, 16.6, 0.0) } else { p + Vec3::new(0.0, -0.75, 0.1) },
            rotation: Vec3::ZERO,
            mass: if is_head { 5.0 } else { 0.5 },
            linear_damping: 0.6,
            angular_damping: 0.8,
            restitution: 0.0,
            friction: 0.5,
            mode: if is_head { RigidMode::FollowBone } else { RigidMode::Physics },
        });
        let cur = (b.m.rigid_bodies.len() - 1) as i32;
        if prev_rb >= 0 {
            b.m.joints.push(Joint {
                name: format!("j {k}"),
                a: prev_rb,
                b: cur,
                position: p,
                rotation: Vec3::ZERO,
                lin_min: Vec3::ZERO,
                lin_max: Vec3::ZERO,
                ang_min: Vec3::splat(-0.6),
                ang_max: Vec3::splat(0.6),
                spring_lin: Vec3::ZERO,
                spring_ang: Vec3::splat(40.0),
            });
        }
        prev_rb = cur;
    }

    // Display frames
    let named = |m: &ModelData, names: &[&str]| -> Vec<(bool, i32)> {
        names.iter().filter_map(|n| m.bone_index(n).map(|i| (false, i as i32))).collect()
    };
    let frames = vec![
        DisplayFrame { name: "Root".into(), special: true, items: vec![(false, root)] },
        DisplayFrame { name: "Expressions".into(), special: true, items: vec![(true, 0), (true, 1)] },
        DisplayFrame { name: "Body".into(), special: false, items: named(&b.m, &["center", "groove", "waist", "upper body", "upper body 2", "lower body", "neck", "head"]) },
        DisplayFrame { name: "Arms".into(), special: false, items: named(&b.m, &["shoulder_L", "arm_L", "elbow_L", "wrist_L", "shoulder_R", "arm_R", "elbow_R", "wrist_R"]) },
        DisplayFrame { name: "Legs".into(), special: false, items: named(&b.m, &["leg IK_L", "leg IK_R", "leg_L", "knee_L", "ankle_L", "leg_R", "knee_R", "ankle_R"]) },
        DisplayFrame { name: "Hair".into(), special: false, items: named(&b.m, &["hair 0", "hair 1", "hair 2", "hair 3"]) },
    ];
    b.m.frames = frames;
    // Recompute smooth normals are already analytic.
    b.m.finalize();
    b.m
}

/// A short procedural dance (keys every few frames) so the timeline shows real keyframes.
pub fn dance() -> Motion {
    let mut m = Motion { model_name: "Demo Mannequin".into(), ..Default::default() };
    let deg = |x: f32, y: f32, z: f32| quat_from_euler_deg(Vec3::new(x, y, z));
    let len = 240u32;
    let step = 10u32;
    let mut f = 0;
    while f <= len {
        let t = f as f32 / 30.0;
        let beat = (t * std::f32::consts::PI * 2.0 * 1.0).sin();
        let half = (t * std::f32::consts::PI * 1.0).sin();
        let curve = [Curve::EASE_IN_OUT; 4];
        let mut key = |name: &str, pos: Vec3, rot: Quat| {
            m.set_bone_key(name, BoneKey { frame: f, pos, rot, curves: curve });
        };
        key("center", Vec3::new(half * 1.2, -0.4 + beat.abs() * -0.5, 0.0), deg(0.0, half * 12.0, 0.0));
        key("upper body", Vec3::ZERO, deg(beat * 4.0, -half * 10.0, half * 6.0));
        key("head", Vec3::ZERO, deg(beat * -6.0, half * 14.0, -half * 8.0));
        key("arm_L", Vec3::ZERO, deg(0.0, 0.0, -30.0 + beat * 35.0));
        key("arm_R", Vec3::ZERO, deg(0.0, 0.0, 30.0 + beat * 35.0));
        key("elbow_L", Vec3::ZERO, deg(0.0, -40.0 - half.abs() * 50.0, 0.0));
        key("elbow_R", Vec3::ZERO, deg(0.0, 40.0 + half.abs() * 50.0, 0.0));
        key("leg IK_L", Vec3::new(half.max(0.0) * 0.6, (beat.max(0.0)) * 1.2, 0.0), Quat::IDENTITY);
        key("leg IK_R", Vec3::new(half.min(0.0) * 0.6, ((-beat).max(0.0)) * 1.2, 0.0), Quat::IDENTITY);
        m.set_morph_key("smile", MorphKey { frame: f, weight: (0.5 + 0.5 * half).clamp(0.0, 1.0) });
        f += step;
    }
    m
}
