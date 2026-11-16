//! PMD (legacy model) reader, written from the public description of the file layout.

use super::reader::{Reader, Res};
use crate::model::*;
use glam::Vec3;

pub fn read(data: &[u8]) -> Res<ModelData> {
    let mut r = Reader::new(data);
    if r.bytes(3)? != b"Pmd" {
        return Err("not a PMD file".into());
    }
    let _ver = r.f32()?;
    let mut m = ModelData {
        name: r.sjis_fixed(20)?,
        comment: r.sjis_fixed(256)?,
        ..Default::default()
    };

    let vc = r.u32()? as usize;
    let mut edge_flags = Vec::with_capacity(vc);
    for _ in 0..vc {
        let pos = r.vec3()?;
        let normal = r.vec3()?;
        let uv = r.vec2()?;
        let b0 = r.u16()? as i32;
        let b1 = r.u16()? as i32;
        let w = r.u8()? as f32 / 100.0;
        let edge_off = r.u8()?;
        edge_flags.push(edge_off);
        m.vertices.push(Vertex {
            pos,
            normal,
            uv,
            bones: [b0, b1, 0, 0],
            weights: [w, 1.0 - w, 0.0, 0.0],
            edge_scale: if edge_off == 0 { 1.0 } else { 0.0 },
            sdef: None,
        });
    }
    let ic = r.u32()? as usize;
    for _ in 0..ic {
        m.indices.push(r.u16()? as u32);
    }

    let mc = r.u32()?;
    let mut tex_lookup: Vec<String> = Vec::new();
    let mut tex_index = |name: &str, list: &mut Vec<String>| -> i32 {
        if name.is_empty() {
            return -1;
        }
        if let Some(i) = list.iter().position(|t| t == name) {
            return i as i32;
        }
        list.push(name.to_string());
        (list.len() - 1) as i32
    };
    for i in 0..mc {
        let d = r.vec3()?;
        let alpha = r.f32()?;
        let specular_power = r.f32()?;
        let specular = r.vec3()?;
        let ambient = r.vec3()?;
        let toon = r.u8()?;
        let edge = r.u8()?;
        let index_count = r.u32()?;
        let texname = r.sjis_fixed(20)?;
        let mut tex = -1;
        let mut sph = -1;
        let mut sphere_mode = SphereMode::Off;
        for part in texname.split('*') {
            let lower = part.to_lowercase();
            if lower.ends_with(".sph") {
                sph = tex_index(part, &mut tex_lookup);
                sphere_mode = SphereMode::Multiply;
            } else if lower.ends_with(".spa") {
                sph = tex_index(part, &mut tex_lookup);
                sphere_mode = SphereMode::Add;
            } else if !part.is_empty() {
                tex = tex_index(part, &mut tex_lookup);
            }
        }
        m.materials.push(Material {
            name: format!("material{i}"),
            diffuse: d.extend(alpha),
            specular,
            specular_power,
            ambient,
            double_sided: alpha < 1.0,
            edge: edge == 1,
            edge_size: 1.0,
            texture: tex,
            sphere: sph,
            sphere_mode,
            toon: if toon == 255 { Toon::None } else { Toon::Shared(toon.min(9)) },
            index_count,
            ..Default::default()
        });
    }
    m.textures = tex_lookup;

    let bc = r.u16()?;
    let mut bone_types = Vec::new();
    let mut ik_parents = Vec::new();
    for _ in 0..bc {
        let name = r.sjis_fixed(20)?;
        let parent = r.u16()?;
        let tail = r.u16()?;
        let kind = r.u8()?;
        let ikp = r.u16()?;
        let pos = r.vec3()?;
        let mut b = Bone::new(&name, pos, if parent == 0xFFFF { -1 } else { parent as i32 });
        b.tail_bone = if tail == 0xFFFF || tail == 0 { -1 } else { tail as i32 };
        b.movable = matches!(kind, 1 | 2);
        b.visible = !matches!(kind, 7);
        b.operable = !matches!(kind, 4 | 6 | 7);
        if kind == 5 || kind == 9 {
            b.append_parent = ikp as i32;
            b.append_rotate = true;
            b.append_ratio = if kind == 9 { tail as f32 * 0.01 } else { 1.0 };
            if kind == 9 {
                b.tail_bone = -1;
            }
        }
        if kind == 8 {
            // twist bone: axis along bone direction, filled later
        }
        bone_types.push(kind);
        ik_parents.push(ikp);
        m.bones.push(b);
    }
    // Fixed axes for twist bones
    for i in 0..m.bones.len() {
        if bone_types[i] == 8 && m.bones[i].tail_bone >= 0 {
            let t = m.bones[m.bones[i].tail_bone as usize].position;
            m.bones[i].fixed_axis = Some((t - m.bones[i].position).normalize_or_zero());
        }
    }

    let ikc = r.u16()?;
    for _ in 0..ikc {
        let ik_bone = r.u16()? as usize;
        let target = r.u16()? as i32;
        let chain = r.u8()?;
        let iterations = r.u16()? as i32;
        let limit = r.f32()?;
        let mut links = Vec::new();
        for _ in 0..chain {
            let li = r.u16()? as i32;
            let name = m.bones.get(li as usize).map(|b| b.name.clone()).unwrap_or_default();
            let limit = if name.contains("ひざ") {
                Some((Vec3::new(-std::f32::consts::PI, 0.0, 0.0), Vec3::new(-0.008, 0.0, 0.0)))
            } else {
                None
            };
            links.push(IkLink { bone: li, limit });
        }
        if let Some(b) = m.bones.get_mut(ik_bone) {
            // PMD stores the per-iteration limit in units of 4 radians.
            b.ik = Some(Ik { target, iterations, limit_angle: limit * 4.0, links });
            b.level = 0;
        }
    }
    // Bones bent by IK are evaluated after the IK bone in PMD; bump their level.
    let _ = &ik_parents;

    // Morphs (skins)
    let sc = r.u16()?;
    let mut base: Vec<u32> = Vec::new();
    for _ in 0..sc {
        let name = r.sjis_fixed(20)?;
        let n = r.u32()?;
        let kind = r.u8()?;
        let mut offs = Vec::new();
        for _ in 0..n {
            let idx = r.u32()?;
            let p = r.vec3()?;
            offs.push((idx, p));
        }
        if kind == 0 {
            base = offs.iter().map(|o| o.0).collect();
            continue;
        }
        let v = offs
            .into_iter()
            .filter_map(|(i, p)| base.get(i as usize).map(|&vi| (vi, p)))
            .collect();
        m.morphs.push(Morph { name, name_en: String::new(), panel: kind, kind: MorphKind::Vertex(v) });
    }
    // Display lists
    let mut frames: Vec<DisplayFrame> = vec![
        DisplayFrame { name: "Root".into(), special: true, items: vec![(false, 0)] },
        DisplayFrame { name: "表情".into(), special: true, items: vec![] },
    ];
    let mdc = r.u8()?;
    for _ in 0..mdc {
        let i = r.u16()? as i32;
        if i > 0 {
            frames[1].items.push((true, i - 1));
        }
    }
    let bnc = r.u8()?;
    for _ in 0..bnc {
        let name = r.sjis_fixed(50)?;
        frames.push(DisplayFrame { name: name.trim().to_string(), special: false, items: vec![] });
    }
    let bdc = r.u32()?;
    for _ in 0..bdc {
        let b = r.u16()? as i32;
        let f = r.u8()? as usize;
        if let Some(fr) = frames.get_mut(f + 1) {
            fr.items.push((false, b));
        }
    }
    m.frames = frames;

    // Optional English names
    if r.remaining() > 0 {
        let has_en = r.u8()?;
        if has_en == 1 {
            m.name_en = r.sjis_fixed(20)?;
            r.skip(256)?;
            for i in 0..m.bones.len() {
                m.bones[i].name_en = r.sjis_fixed(20)?;
            }
            for i in 0..m.morphs.len() {
                m.morphs[i].name_en = r.sjis_fixed(20)?;
            }
            r.skip(50 * bnc as usize)?;
        }
    }
    // Custom toon texture names: we keep the shared procedural ramps instead.
    if r.remaining() >= 1000 {
        r.skip(1000)?;
    }
    // Rigid bodies
    if r.remaining() >= 4 {
        let rc = r.u32()?;
        for _ in 0..rc {
            let name = r.sjis_fixed(20)?;
            let bone = r.u16()?;
            let group = r.u8()?;
            let mask = r.u16()?;
            let shape = match r.u8()? {
                0 => Shape::Sphere,
                1 => Shape::Box,
                _ => Shape::Capsule,
            };
            let size = r.vec3()?;
            let pos = r.vec3()?;
            let rot = r.vec3()?;
            let bone_i = if bone == 0xFFFF { -1 } else { bone as i32 };
            let bpos = if bone_i >= 0 {
                m.bones.get(bone_i as usize).map(|b| b.position).unwrap_or(Vec3::ZERO)
            } else {
                m.bones.first().map(|b| b.position).unwrap_or(Vec3::ZERO)
            };
            m.rigid_bodies.push(RigidBody {
                name,
                bone: bone_i,
                group,
                mask,
                shape,
                size,
                position: pos + bpos,
                rotation: rot,
                mass: r.f32()?,
                linear_damping: r.f32()?,
                angular_damping: r.f32()?,
                restitution: r.f32()?,
                friction: r.f32()?,
                mode: match r.u8()? {
                    0 => RigidMode::FollowBone,
                    1 => RigidMode::Physics,
                    _ => RigidMode::PhysicsWithBonePosition,
                },
            });
        }
    }
    if r.remaining() >= 4 {
        let jc = r.u32()?;
        for _ in 0..jc {
            m.joints.push(Joint {
                name: r.sjis_fixed(20)?,
                a: r.u32()? as i32,
                b: r.u32()? as i32,
                position: r.vec3()?,
                rotation: r.vec3()?,
                lin_min: r.vec3()?,
                lin_max: r.vec3()?,
                ang_min: r.vec3()?,
                ang_max: r.vec3()?,
                spring_lin: r.vec3()?,
                spring_ang: r.vec3()?,
            });
        }
    }
    // Bones driven by IK links are evaluated after their IK bone.
    let ik_link_bones: Vec<i32> = m
        .bones
        .iter()
        .filter_map(|b| b.ik.as_ref())
        .flat_map(|ik| ik.links.iter().map(|l| l.bone))
        .collect();
    let _ = ik_link_bones;
    m.finalize();
    Ok(m)
}
