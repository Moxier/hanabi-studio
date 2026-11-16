//! PMX 2.0 / 2.1 model reader, written from the public description of the file layout.

use super::reader::{Reader, Res};
use crate::model::*;
use glam::Vec3;

struct Globals {
    utf8: bool,
    extra_uv: u8,
    vertex_index: u8,
    texture_index: u8,
    material_index: u8,
    bone_index: u8,
    morph_index: u8,
    rigid_index: u8,
}

fn text(r: &mut Reader, g: &Globals) -> Res<String> {
    let len = r.i32()?;
    if len <= 0 {
        return Ok(String::new());
    }
    let b = r.bytes(len as usize)?;
    if g.utf8 {
        Ok(String::from_utf8_lossy(b).into_owned())
    } else {
        let u: Vec<u16> = b.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        Ok(String::from_utf16_lossy(&u))
    }
}

pub fn read(data: &[u8]) -> Res<ModelData> {
    let mut r = Reader::new(data);
    let magic = r.bytes(4)?;
    if &magic[..3] != b"PMX" {
        return Err("not a PMX file".into());
    }
    let _version = r.f32()?;
    let n = r.u8()? as usize;
    let gb = r.bytes(n)?;
    let gv = |i: usize, d: u8| gb.get(i).copied().unwrap_or(d);
    let g = Globals {
        utf8: gv(0, 0) == 1,
        extra_uv: gv(1, 0),
        vertex_index: gv(2, 4),
        texture_index: gv(3, 4),
        material_index: gv(4, 4),
        bone_index: gv(5, 4),
        morph_index: gv(6, 4),
        rigid_index: gv(7, 4),
    };
    let mut m = ModelData {
        name: text(&mut r, &g)?,
        name_en: text(&mut r, &g)?,
        comment: text(&mut r, &g)?,
        ..Default::default()
    };
    let _comment_en = text(&mut r, &g)?;

    // Vertices
    let vc = r.i32()?.max(0) as usize;
    m.vertices.reserve(vc);
    for _ in 0..vc {
        let pos = r.vec3()?;
        let normal = r.vec3()?;
        let uv = r.vec2()?;
        for _ in 0..g.extra_uv {
            r.vec4()?;
        }
        let kind = r.u8()?;
        let mut v = Vertex {
            pos,
            normal,
            uv,
            bones: [0; 4],
            weights: [0.0; 4],
            edge_scale: 1.0,
            sdef: None,
        };
        let bi = g.bone_index;
        match kind {
            0 => {
                v.bones[0] = r.index(bi, false)?;
                v.weights[0] = 1.0;
            }
            1 => {
                v.bones[0] = r.index(bi, false)?;
                v.bones[1] = r.index(bi, false)?;
                let w = r.f32()?;
                v.weights[0] = w;
                v.weights[1] = 1.0 - w;
            }
            2 | 4 => {
                for i in 0..4 {
                    v.bones[i] = r.index(bi, false)?;
                }
                for i in 0..4 {
                    v.weights[i] = r.f32()?;
                }
            }
            3 => {
                v.bones[0] = r.index(bi, false)?;
                v.bones[1] = r.index(bi, false)?;
                let w = r.f32()?;
                v.weights[0] = w;
                v.weights[1] = 1.0 - w;
                let c = r.vec3()?;
                let r0 = r.vec3()?;
                let r1 = r.vec3()?;
                v.sdef = Some([c, r0, r1]);
            }
            k => return Err(format!("unknown vertex weight type {k}")),
        }
        v.edge_scale = r.f32()?;
        m.vertices.push(v);
    }

    // Faces
    let ic = r.i32()?.max(0) as usize;
    m.indices.reserve(ic);
    for _ in 0..ic {
        m.indices.push(r.index(g.vertex_index, true)? as u32);
    }

    // Textures
    let tc = r.i32()?.max(0);
    for _ in 0..tc {
        m.textures.push(text(&mut r, &g)?.replace('\\', "/"));
    }

    // Materials
    let mc = r.i32()?.max(0);
    for _ in 0..mc {
        let name = text(&mut r, &g)?;
        let name_en = text(&mut r, &g)?;
        let diffuse = r.vec4()?;
        let specular = r.vec3()?;
        let specular_power = r.f32()?;
        let ambient = r.vec3()?;
        let flags = r.u8()?;
        let edge_color = r.vec4()?;
        let edge_size = r.f32()?;
        let texture = r.index(g.texture_index, false)?;
        let sphere = r.index(g.texture_index, false)?;
        let sphere_mode = match r.u8()? {
            1 => SphereMode::Multiply,
            2 => SphereMode::Add,
            3 => SphereMode::SubTexture,
            _ => SphereMode::Off,
        };
        let shared = r.u8()?;
        let toon = if shared == 1 {
            Toon::Shared(r.u8()?)
        } else {
            let t = r.index(g.texture_index, false)?;
            if t < 0 {
                Toon::None
            } else {
                Toon::Texture(t)
            }
        };
        let memo = text(&mut r, &g)?;
        let index_count = r.i32()?.max(0) as u32;
        m.materials.push(Material {
            name,
            name_en,
            diffuse,
            specular,
            specular_power,
            ambient,
            double_sided: flags & 1 != 0,
            ground_shadow: flags & 2 != 0,
            cast_shadow: flags & 4 != 0,
            receive_shadow: flags & 8 != 0,
            edge: flags & 16 != 0,
            edge_color,
            edge_size,
            texture,
            sphere,
            sphere_mode,
            toon,
            index_count,
            memo,
        });
    }

    // Bones
    let bc = r.i32()?.max(0);
    for _ in 0..bc {
        let name = text(&mut r, &g)?;
        let name_en = text(&mut r, &g)?;
        let position = r.vec3()?;
        let parent = r.index(g.bone_index, false)?;
        let level = r.i32()?;
        let flags = r.u16()?;
        let mut b = Bone::new(&name, position, parent);
        b.name_en = name_en;
        b.level = level;
        b.rotatable = flags & 0x0002 != 0;
        b.movable = flags & 0x0004 != 0;
        b.visible = flags & 0x0008 != 0;
        b.operable = flags & 0x0010 != 0;
        b.after_physics = flags & 0x1000 != 0;
        if flags & 0x0001 != 0 {
            b.tail_bone = r.index(g.bone_index, false)?;
        } else {
            b.tail_offset = r.vec3()?;
        }
        if flags & (0x0100 | 0x0200) != 0 {
            b.append_parent = r.index(g.bone_index, false)?;
            b.append_ratio = r.f32()?;
            b.append_rotate = flags & 0x0100 != 0;
            b.append_translate = flags & 0x0200 != 0;
            b.append_local = flags & 0x0080 != 0;
        }
        if flags & 0x0400 != 0 {
            b.fixed_axis = Some(r.vec3()?.normalize_or_zero());
        }
        if flags & 0x0800 != 0 {
            let x = r.vec3()?;
            let z = r.vec3()?;
            b.local_axes = Some((x, z));
        }
        if flags & 0x2000 != 0 {
            r.i32()?;
        }
        if flags & 0x0020 != 0 {
            let target = r.index(g.bone_index, false)?;
            let iterations = r.i32()?;
            let limit_angle = r.f32()?;
            let lc = r.i32()?.max(0);
            let mut links = Vec::new();
            for _ in 0..lc {
                let bone = r.index(g.bone_index, false)?;
                let has = r.u8()?;
                let limit = if has == 1 {
                    let a = r.vec3()?;
                    let c = r.vec3()?;
                    Some((a.min(c), a.max(c)))
                } else {
                    None
                };
                links.push(IkLink { bone, limit });
            }
            b.ik = Some(Ik { target, iterations, limit_angle, links });
        }
        m.bones.push(b);
    }

    // Morphs
    let morph_count = r.i32()?.max(0);
    for _ in 0..morph_count {
        let name = text(&mut r, &g)?;
        let name_en = text(&mut r, &g)?;
        let panel = r.u8()?;
        let kind = r.u8()?;
        let oc = r.i32()?.max(0);
        let k = match kind {
            0 => {
                let mut v = Vec::new();
                for _ in 0..oc {
                    v.push((r.index(g.morph_index, false)?, r.f32()?));
                }
                MorphKind::Group(v)
            }
            1 => {
                let mut v = Vec::new();
                for _ in 0..oc {
                    v.push((r.index(g.vertex_index, true)? as u32, r.vec3()?));
                }
                MorphKind::Vertex(v)
            }
            2 => {
                let mut v = Vec::new();
                for _ in 0..oc {
                    v.push((r.index(g.bone_index, false)?, r.vec3()?, r.quat()?));
                }
                MorphKind::Bone(v)
            }
            3..=7 => {
                let mut v = Vec::new();
                for _ in 0..oc {
                    v.push((r.index(g.vertex_index, true)? as u32, r.vec4()?));
                }
                if kind == 3 {
                    MorphKind::Uv(v)
                } else {
                    MorphKind::Uv(Vec::new())
                }
            }
            8 => {
                let mut v = Vec::new();
                for _ in 0..oc {
                    v.push(MaterialMorphOffset {
                        material: r.index(g.material_index, false)?,
                        additive: r.u8()? == 1,
                        diffuse: r.vec4()?,
                        specular: r.vec3()?,
                        specular_power: r.f32()?,
                        ambient: r.vec3()?,
                        edge_color: r.vec4()?,
                        edge_size: r.f32()?,
                        texture_tint: r.vec4()?,
                        sphere_tint: r.vec4()?,
                        toon_tint: r.vec4()?,
                    });
                }
                MorphKind::Material(v)
            }
            9 => {
                let mut v = Vec::new();
                for _ in 0..oc {
                    v.push((r.index(g.morph_index, false)?, r.f32()?));
                }
                MorphKind::Flip(v)
            }
            10 => {
                let mut v = Vec::new();
                for _ in 0..oc {
                    v.push((r.index(g.rigid_index, false)?, r.u8()? == 1, r.vec3()?, r.vec3()?));
                }
                MorphKind::Impulse(v)
            }
            k => return Err(format!("unknown morph type {k}")),
        };
        m.morphs.push(Morph { name, name_en, panel, kind: k });
    }

    // Display frames
    let fc = r.i32()?.max(0);
    for _ in 0..fc {
        let name = text(&mut r, &g)?;
        let _en = text(&mut r, &g)?;
        let special = r.u8()? == 1;
        let ec = r.i32()?.max(0);
        let mut items = Vec::new();
        for _ in 0..ec {
            let t = r.u8()?;
            if t == 0 {
                items.push((false, r.index(g.bone_index, false)?));
            } else {
                items.push((true, r.index(g.morph_index, false)?));
            }
        }
        m.frames.push(DisplayFrame { name, special, items });
    }

    // Rigid bodies (missing in some stripped files, so stop gracefully)
    if r.remaining() >= 4 {
        let rc = r.i32()?.max(0);
        for _ in 0..rc {
            let name = text(&mut r, &g)?;
            let _en = text(&mut r, &g)?;
            let bone = r.index(g.bone_index, false)?;
            let group = r.u8()?;
            let mask = r.u16()?;
            let shape = match r.u8()? {
                0 => Shape::Sphere,
                1 => Shape::Box,
                _ => Shape::Capsule,
            };
            m.rigid_bodies.push(RigidBody {
                name,
                bone,
                group,
                mask,
                shape,
                size: r.vec3()?,
                position: r.vec3()?,
                rotation: r.vec3()?,
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
        let jc = r.i32()?.max(0);
        for _ in 0..jc {
            let name = text(&mut r, &g)?;
            let _en = text(&mut r, &g)?;
            let _kind = r.u8()?;
            m.joints.push(Joint {
                name,
                a: r.index(g.rigid_index, false)?,
                b: r.index(g.rigid_index, false)?,
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
    // PMX 2.1 soft bodies are not simulated; they are ignored.
    let _ = Vec3::ZERO;
    m.finalize();
    Ok(m)
}
